/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `xarxa server start` — start the Xarxa server.
//!
//! ```bash
//! xarxa server start          # local server: SQLite + wasmtime, no JWT
//! ```

use anyhow::Result;
use axum::{
    extract::{Json, Path, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use clap::{Args, Subcommand};
use colored::Colorize;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;

use crate::adapters::wasmtime::wit_runtime::WitPluginRuntime;

#[derive(Subcommand)]
pub enum ServerCommand {
    /// Start the server (SQLite, no JWT, wasmtime execution)
    Start(StartArgs),
}

#[derive(Args)]
pub struct StartArgs {
    /// Port to listen on
    #[arg(long, short, default_value = "4000", env = "PORT")]
    pub port: u16,

    /// Directory to watch and load .wasm plugins from
    #[arg(long, default_value = "./plugins", env = "WORKFLOWS_DEFAULT_PATH")]
    pub plugins_dir: String,

    /// SQLite database path
    #[arg(long, default_value = "./xarxa.db", env = "XARXA_DB")]
    pub db: String,
}

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    runtime: Arc<WitPluginRuntime>,
}

pub async fn run(cmd: ServerCommand) -> Result<()> {
    match cmd {
        ServerCommand::Start(args) => start(args).await,
    }
}

pub async fn start(args: StartArgs) -> Result<()> {
    println!("{}", "  Xarxa Server".bold().cyan());
    println!("  {} http://localhost:{}", "listening:".dimmed(), args.port);
    println!("  {} {}", "plugins: ".dimmed(), args.plugins_dir);
    println!("  {} {}", "database:".dimmed(), args.db);
    println!();

    // ── SQLite ────────────────────────────────────────────────────────────────
    let db_url = format!("sqlite://{}?mode=rwc", args.db);
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    sqlx::migrate!("./migrations/sqlite")
        .run(&db)
        .await?;

    // ── WitPluginRuntime ─────────────────────────────────────────────────────
    let runtime = Arc::new(WitPluginRuntime::new()?);

    let dir = std::path::PathBuf::from(&args.plugins_dir);
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let bytes = std::fs::read(&path)?;
                runtime.load_wit_plugin(&name, &bytes).await?;
                println!("  {} plugin '{}' ({} bytes)", "✓".green(), name, bytes.len());
            }
        }
    }
    println!();

    // ── Hot-reload watcher ────────────────────────────────────────────────────
    {
        let runtime = runtime.clone();
        let dir = dir.clone();
        tokio::spawn(async move { watch_plugins(runtime, dir).await });
    }

    let state = AppState { db, runtime };

    let app = Router::new()
        .route("/api/workflows/{name}/{function}", post(handle_workflow))
        .route("/api/plugins", get(list_plugins))
        .route("/api/runs", get(list_runs))
        .route("/health", get(|| async { "ok" }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", args.port)).await?;
    println!("  {} ready — Ctrl+C to stop", "✓".green().bold());
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_workflow(
    State(state): State<AppState>,
    Path((name, function)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Extract the actual workflow input — the body may carry extra envelope fields
    // (workflow_name, run_id, etc.) sent by the client. We want only `input`.
    // If the body has an "input" field, use it; otherwise treat the whole body as input.
    let input_value: serde_json::Value = body.get("input")
        .cloned()
        .unwrap_or_else(|| body.clone());
    // Normalise to a plain JSON string so the plugin always gets raw bytes
    let input_str: String = match &input_value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    // Insert run record
    let run_id: i64 = match sqlx::query_scalar(
        "INSERT INTO workflow_runs (plugin, input) VALUES (?, ?) RETURNING id",
    )
    .bind(&name)
    .bind(&input_str)
    .fetch_one(&state.db)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            return axum::Json(serde_json::json!({
                "success": false,
                "error": format!("DB error: {}", e)
            }))
        }
    };

    // Checkpoint callback: on every send-result from the plugin, persist the
    // state blob and append an event. This is what enables crash recovery —
    // the last state blob is passed back via continue-workflow on resume.
    let db_cb = state.db.clone();
    let rid = run_id;
    // Note: `_plugin_id` is the id the plugin passes to send-result — currently
    // always 0 because the WIT contract doesn't carry our SQLite run_id.
    // We use `rid` (the real SQLite id) captured in the closure instead.
    let on_result = Arc::new(move |_plugin_id: u64, data: Vec<u8>| {
        let db = db_cb.clone();
        let data_clone = data.clone();
        tokio::spawn(async move {
            let seq: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM workflow_events WHERE run_id = ?",
            )
            .bind(rid)
            .fetch_one(&db)
            .await
            .unwrap_or(1);

            let _ = sqlx::query(
                "INSERT INTO workflow_events (run_id, seq, data) VALUES (?, ?, ?)",
            )
            .bind(rid)
            .bind(seq)
            .bind(&data_clone)
            .execute(&db)
            .await;

            let _ = sqlx::query(
                "UPDATE workflow_runs SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(&data_clone)
            .bind(rid)
            .execute(&db)
            .await;
        });
    });

    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Execute via wasmtime
    let params = serde_json::json!({
        "workflow_name": name,
        "input": input_str,
        "run_id": run_id,
        "started_at": started_at,
    });

    let result = state.runtime
        .execute_wit_function_with_hook(&name, &function, params, Some(on_result))
        .await;

    match result {
        Ok(output) => {
            let output_str = output.to_string();
            let _ = sqlx::query(
                "UPDATE workflow_runs SET output = ?, status = 'completed' WHERE id = ?",
            )
            .bind(&output_str)
            .bind(run_id)
            .execute(&state.db)
            .await;

            axum::Json(serde_json::json!({
                "success": true,
                "run_id": run_id,
                "plugin": name,
                "function": function,
                "output": output,
            }))
        }
        Err(e) => {
            let _ = sqlx::query(
                "UPDATE workflow_runs SET output = ?, status = 'failed' WHERE id = ?",
            )
            .bind(e.to_string())
            .bind(run_id)
            .execute(&state.db)
            .await;

            axum::Json(serde_json::json!({
                "success": false,
                "run_id": run_id,
                "error": e.to_string(),
            }))
        }
    }
}

async fn list_plugins(State(state): State<AppState>) -> impl IntoResponse {
    let names = state.runtime.list_plugin_names().await;
    axum::Json(serde_json::json!({ "plugins": names }))
}

async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    let rows: Vec<(i64, String, Option<String>, String, String)> = match sqlx::query_as(
        "SELECT id, plugin, output, status, created_at FROM workflow_runs ORDER BY id DESC LIMIT 100",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return axum::Json(serde_json::json!({ "error": e.to_string() }))
        }
    };

    let runs: Vec<_> = rows
        .into_iter()
        .map(|(id, plugin, output, status, created_at)| {
            serde_json::json!({
                "id": id,
                "plugin": plugin,
                "output": output,
                "status": status,
                "created_at": created_at,
            })
        })
        .collect();

    axum::Json(serde_json::json!({ "runs": runs }))
}

async fn watch_plugins(runtime: Arc<WitPluginRuntime>, dir: std::path::PathBuf) {
    use notify::{EventKind, RecursiveMode, Watcher};
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(_) => return,
    };
    if dir.exists() {
        let _ = watcher.watch(&dir, RecursiveMode::NonRecursive);
    }

    for event in rx.into_iter().flatten() {
        if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
            for path in event.paths {
                if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let name = path.file_stem().unwrap().to_string_lossy().to_string();
                        if let Err(e) = runtime.load_wit_plugin(&name, &bytes).await {
                            eprintln!("  {} failed to reload '{}': {}", "✗".red(), name, e);
                        } else {
                            println!("  {} plugin '{}' reloaded", "↻".cyan(), name);
                        }
                    }
                }
            }
        }
    }
}
