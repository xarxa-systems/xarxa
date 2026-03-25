/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * Xarxa is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
 * See the LICENSE file in the project root for the full license text.
 *
 * Commercial licensing (MIT / proprietary) is available.
 * Contact: contact@xarxa.io
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use anyhow::Result;
use clap::{Parser, Subcommand};

mod core;
mod adapters;
mod infra;
mod commands;

#[derive(Parser)]
#[command(
    name = "xarxa",
    version = env!("CARGO_PKG_VERSION"),
    about = "Xarxa — Durable WASM Workflow Orchestrator",
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the Xarxa server
    #[command(subcommand)]
    Server(commands::server::ServerCommand),

    /// Interact with workflows on a running server
    #[command(subcommand)]
    Workflow(commands::workflow::WorkflowCommand),

    /// Manage workflow WASM plugins
    #[command(subcommand)]
    Plugin(commands::plugin::PluginCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Command::Server(cmd) => commands::server::run(cmd).await,
        Command::Workflow(cmd) => commands::workflow::run(cmd).await,
        Command::Plugin(cmd) => commands::plugin::run(cmd).await,
    }
}

/// Full production server bootstrap (PostgreSQL + S3 + wasmtime).
async fn run_server() -> Result<()> {
    use std::path::PathBuf;
    use std::sync::Arc;
    use tracing::{error, info, Level};
    use tracing_subscriber::EnvFilter;
    use sqlx::postgres::PgPoolOptions;
    use aws_sdk_s3 as s3;
    use s3::config::Region;

    use infra::config::AppConfig;
    use adapters::http;
    use adapters::postgres::{
        user_repo::PostgresUserRepository,
        workflow_repo::PostgresWorkflowRepository,
        namespace_repo::PostgresNamespaceRepository,
    };
    use adapters::wasmtime::wit_runtime::WitPluginRuntime;
    use adapters::filesystem::plugin_auto_loader::PluginAutoLoader;
    use core::services::{
        user::UserService,
        workflow::WorkflowService,
        namespace::NamespaceService,
    };

    let config = Arc::new(AppConfig::from_env()?);

    tracing_subscriber::fmt()
        .with_max_level(config.log_level.parse::<Level>()?)
        .with_env_filter(EnvFilter::new(
            "xarxa=debug,cranelift=warn,tower_http=debug,axum::routing=trace,hyper=info",
        ))
        .init();

    info!("Starting Xarxa...");

    let aws_configuration = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .credentials_provider((*config).clone())
        .region(Region::new("ams3"))
        .endpoint_url(&config.space_endpoint)
        .load()
        .await;

    let s3_client = Arc::new(s3::Client::new(&aws_configuration));

    let pool = Arc::new(
        PgPoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?,
    );

    let wit_runtime = Arc::new(WitPluginRuntime::new()?);

    let workflows_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));
    let users_repo = Arc::new(PostgresUserRepository::new(pool.clone()));
    let namespace_repo = Arc::new(PostgresNamespaceRepository::new(pool.clone()));

    let workflows_service = Arc::new(WorkflowService::new(
        workflows_repo.clone(),
        s3_client.clone(),
        wit_runtime.clone(),
    ));
    let user_service = Arc::new(UserService::new(users_repo.clone()));
    let namespace_service = Arc::new(NamespaceService::new(namespace_repo));

    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (loader_shutdown_tx, loader_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let http_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("http-server")
        .max_blocking_threads(4)
        .enable_all()
        .build()?;

    let wit_to_http = wit_runtime.clone();
    let wit_to_loader = wit_runtime.clone();
    let http_config = config.clone();

    let http_handle = http_runtime.spawn(async move {
        if let Err(e) = http::start_server(
            s3_client,
            workflows_service,
            user_service,
            namespace_service,
            http_shutdown_rx,
            wit_to_http,
            http_config,
        )
        .await
        {
            error!("HTTP server error: {}", e);
        }
    });

    let loader_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("wasm-loader")
        .max_blocking_threads(4)
        .enable_all()
        .build()?;

    let mut loader = PluginAutoLoader::new(
        wit_to_loader,
        loader_shutdown_rx,
        PathBuf::from(config.workflows_default_path.clone()),
    );

    let loader_handle = loader_runtime.spawn(async move {
        if let Err(e) = loader.start().await {
            error!("Loader error: {}", e);
        }
    });

    info!("HTTP server:   http://localhost:{}", config.port);
    info!("Plugins dir:   {}", config.workflows_default_path);
    info!("Press Ctrl+C to stop");

    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    let _ = http_shutdown_tx.send(());
    let _ = loader_shutdown_tx.send(());

    let timeout = tokio::time::Duration::from_secs(10);
    match tokio::time::timeout(timeout, async { tokio::join!(http_handle, loader_handle) }).await {
        Ok((http_res, loader_res)) => {
            if let Err(e) = http_res { error!("HTTP task: {}", e); }
            if let Err(e) = loader_res { error!("Loader task: {}", e); }
        }
        Err(_) => {
            error!("Shutdown timeout, forcing exit");
            std::process::exit(1);
        }
    }

    http_runtime.shutdown_background();
    loader_runtime.shutdown_background();

    info!("Xarxa stopped");
    Ok(())
}
