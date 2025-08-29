use tracing::{error, info, Level};
use tracing_subscriber::EnvFilter;
use std::path::PathBuf;
use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;

use infra::config::AppConfig;
use adapters::http;

use aws_sdk_s3 as s3;
use s3::config::Region;
use aws_config;

mod core;
mod adapters;
mod infra;

use crate::adapters::postgres::namespace_repo::PostgresNamespaceRepository;
use crate::adapters::wasmtime::wit_runtime::WitPluginRuntime;
use crate::adapters::filesystem::plugin_auto_loader::PluginAutoLoader;

use crate::core::services::namespace::NamespaceService;
use crate::core::services::{
    user::UserService,
    workflow::WorkflowService,
};

use crate::adapters::postgres::{
    user_repo::PostgresUserRepository,
    workflow_repo::PostgresWorkflowRepository,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(AppConfig::from_env()?);

    tracing_subscriber::fmt()
        .with_max_level(config.log_level.parse::<Level>()?)
        .with_env_filter(EnvFilter::new("xarxa=debug,cranelift=warn,tower_http=debug,axum::routing=trace,hyper=info"))
        .init();

    info!("Starting Xarxa...");

    let s3_config = config.clone();
    let http_config = config.clone();

    tracing::info!("Running on port: {}", config.port);

    let aws_configuration = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .credentials_provider((*s3_config).clone())
        .region(Region::new("ams3"))
        .endpoint_url(&config.space_endpoint)
        .load().await;

    let s3_client = Arc::new(s3::Client::new(&aws_configuration));

    let pool = Arc::new(PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url.to_owned())
        .await?);

    // --- wit runtime
    let wit_runtime = Arc::new(WitPluginRuntime::new()?);
    // --- wit runtime end
    
    // --- repos ---
    let workflows_repo = Arc::new(PostgresWorkflowRepository::new(pool.clone()));
    let users_repo = Arc::new(PostgresUserRepository::new(pool.clone()));
    let namespace_repo = Arc::new(PostgresNamespaceRepository::new(pool.clone()));
    // --- end repos ---

    // --- services ---
    let workflows_service = Arc::new(WorkflowService::new(workflows_repo.clone(), s3_client.clone(), wit_runtime.clone()));
    let user_service = Arc::new(UserService::new(users_repo.clone()));
    let namespace_service = Arc::new(NamespaceService::new(namespace_repo));
    // --- end services ---

    // The channels for graceful shutdown
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (loader_shutdown_tx, loader_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Run the HTTP server in separate runtime environment.
    let http_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("http-server")
        .max_blocking_threads(4)
        .enable_all()
        .build()?;

    let wit_to_http = wit_runtime.clone();
    let wit_to_loader = wit_runtime.clone();

    let http_handle = http_runtime.spawn(async move {
        if let Err(e) = http::start_server(
            s3_client.clone(), 
            workflows_service.clone(), 
            user_service.clone(), 
            namespace_service.clone(),
            http_shutdown_rx, 
            wit_to_http, 
            http_config,
        ).await {
            error!("HTTP server error: {}", e);
        }
    });

    let mut loader = PluginAutoLoader::new(
        wit_to_loader,
        loader_shutdown_rx,
        PathBuf::from(config.workflows_default_path.clone())
    );

    let loader_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("wasm-loader")
        .max_blocking_threads(4)
        .enable_all()
        .build()?;

    let loader_handle = loader_runtime.spawn(async move {
        if let Err(e) = loader.start().await {
            error!("Loader error: {}", e);
        }
    });

    info!("All runtimes started successfully");
    info!("HTTP server: http://localhost:4000");
    info!("Engine: Ready to execute WASM functions");
    info!("Plugin system: Ready with typed serialization");
    info!("Press Ctrl+C to shutdown gracefully");

    // Wait shutdown signal.
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal, initiating graceful shutdown...");

    // Send shutdown signal to all components
    let _ = http_shutdown_tx.send(());
    let _ = loader_shutdown_tx.send(());

    let shutdown_timeout = tokio::time::Duration::from_secs(10);

    match tokio::time::timeout(shutdown_timeout, async {
        tokio::join!(http_handle, loader_handle)
    }).await {
        Ok(( http_result, loader)) => {
            info!("All components shutdown gracefully");
            
            if let Err(e) = loader {
                error!("Engine task failed: {}", e);
            }
            if let Err(e) = http_result {
                error!("HTTP server task failed: {}", e);
            }
        },
        Err(_) => {
            error!("Shutdown timeout reached, forcing exit");
            std::process::exit(1);
        }
    }

    http_runtime.shutdown_background();
    loader_runtime.shutdown_background();

    info!("Xarxa kernel shutdown complete");
    Ok(())
}
