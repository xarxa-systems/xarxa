/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `xarxa workflow run` — run a workflow.
//!
//! By default runs **directly via wasmtime** — no server, no auth needed.
//! Pass `--server` to send to a running Xarxa server instead.
//!
//! ```bash
//! # Interactive: picks plugin from plugins/, prompts for input
//! xarxa workflow run
//!
//! # Direct wasmtime (default, no server needed)
//! xarxa workflow run booking --input '{"flight_id":"AA123","amount":99}'
//!
//! # Against a remote server
//! xarxa workflow run booking --input '...' --server http://prod:4000 --token $JWT
//! ```

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Subcommand)]
pub enum WorkflowCommand {
    /// Run a workflow (locally via wasmtime by default, or against a server)
    Run(RunArgs),
}

#[derive(Args)]
pub struct RunArgs {
    /// Workflow plugin name. If omitted, picks from --plugins-dir interactively.
    #[arg(value_name = "WORKFLOW")]
    pub workflow: Option<String>,

    /// JSON input for start-workflow
    #[arg(long, value_name = "JSON", conflicts_with = "continue_id")]
    pub input: Option<String>,

    /// Run ID to continue (triggers continue-workflow)
    #[arg(long = "continue", value_name = "RUN_ID", conflicts_with = "input")]
    pub continue_id: Option<u64>,

    /// Directory containing .wasm files (local runs)
    #[arg(long, default_value = "./plugins", env = "WORKFLOWS_DEFAULT_PATH")]
    pub plugins_dir: PathBuf,

    /// Send to a remote Xarxa server instead of running locally.
    /// Example: --server http://localhost:4000
    #[arg(long, env = "XARXA_SERVER")]
    pub server: Option<String>,

    /// Bearer token for auth (only used with --server)
    #[arg(long, env = "XARXA_TOKEN")]
    pub token: Option<String>,

    /// SQLite database path (shared with `xarxa server start`)
    #[arg(long, default_value = "./xarxa.db", env = "XARXA_DB")]
    pub db: PathBuf,

    /// Pretty-print JSON response
    #[arg(long, short)]
    pub pretty: bool,
}

pub async fn run(cmd: WorkflowCommand) -> Result<()> {
    match cmd {
        WorkflowCommand::Run(args) => run_workflow(args).await,
    }
}

async fn run_workflow(mut args: RunArgs) -> Result<()> {
    if args.workflow.is_none() {
        args.workflow = Some(pick_plugin(&args.plugins_dir)?);
    }

    if args.input.is_none() && args.continue_id.is_none() {
        args.input = Some(prompt_input()?);
    }

    if args.server.is_some() {
        run_remote(args).await
    } else {
        run_local(args).await
    }
}

// ── Local: wasmtime directly + SQLite persistence ────────────────────────────

async fn open_db(path: &PathBuf) -> Result<SqlitePool> {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .with_context(|| format!("Cannot open SQLite at '{}'", path.display()))?;
    sqlx::migrate!("./migrations/sqlite").run(&pool).await?;
    Ok(pool)
}

async fn run_local(args: RunArgs) -> Result<()> {
    use crate::adapters::wasmtime::wit_runtime::WitPluginRuntime;

    let name = args.workflow.as_deref().unwrap();
    let db = open_db(&args.db).await?;

    // ── continue-workflow ─────────────────────────────────────────────────────
    if let Some(run_id) = args.continue_id {
        let row: Option<(String, Option<Vec<u8>>, String)> = sqlx::query_as(
            "SELECT plugin, state, status FROM workflow_runs WHERE id = ?",
        )
        .bind(run_id as i64)
        .fetch_optional(&db)
        .await?;

        let (plugin, state, status) = row
            .with_context(|| format!("Run #{} not found", run_id))?;

        if status == "completed" {
            bail!("Run #{} already completed", run_id);
        }

        let state_bytes = state.unwrap_or_default();

        let wasm_path = args.plugins_dir.join(format!("{}.wasm", plugin));
        if !wasm_path.exists() {
            bail!("Plugin '{}' not found at '{}'", plugin, wasm_path.display());
        }

        println!("{}", "  Xarxa Continue (local)".bold().cyan());
        println!("  {} {}", "plugin: ".dimmed(), plugin);
        println!("  {} #{}", "run:    ".dimmed(), run_id);
        println!();

        let wasm_bytes = std::fs::read(&wasm_path)?;
        let runtime = WitPluginRuntime::new()?;
        runtime.load_wit_plugin(&plugin, &wasm_bytes).await?;

        let (db_cb, rid) = (db.clone(), run_id as i64);
        let on_result = Arc::new(move |_id: u64, data: Vec<u8>| {
            let db = db_cb.clone();
            let data = data.clone();
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
                .bind(rid).bind(seq).bind(&data)
                .execute(&db).await;

                let _ = sqlx::query(
                    "UPDATE workflow_runs SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                )
                .bind(&data).bind(rid)
                .execute(&db).await;
            });
        });

        let params = serde_json::json!({ "run_id": run_id, "state": state_bytes });
        let result = runtime
            .execute_wit_function_with_hook(&plugin, "continue-workflow", params, Some(on_result))
            .await;

        return finish_run(&db, run_id as i64, result, args.pretty).await;
    }

    // ── start-workflow ────────────────────────────────────────────────────────
    let input_str = args.input.as_deref().unwrap_or("{}");
    serde_json::from_str::<serde_json::Value>(input_str)
        .with_context(|| format!("Invalid --input JSON: {}", input_str))?;

    let wasm_path = args.plugins_dir.join(format!("{}.wasm", name));
    if !wasm_path.exists() {
        bail!(
            "Plugin '{}' not found at '{}'\nRun: xarxa plugin build --out {}",
            name, wasm_path.display(), args.plugins_dir.display(),
        );
    }

    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO workflow_runs (plugin, input) VALUES (?, ?) RETURNING id",
    )
    .bind(name)
    .bind(input_str)
    .fetch_one(&db)
    .await?;

    println!("{}", "  Xarxa Run (local)".bold().cyan());
    println!("  {} {}", "plugin: ".dimmed(), name);
    println!("  {} {}", "input:  ".dimmed(), input_str);
    println!("  {} #{}", "run:    ".dimmed(), run_id);
    println!();

    let wasm_bytes = std::fs::read(&wasm_path)
        .with_context(|| format!("Cannot read '{}'", wasm_path.display()))?;

    let runtime = WitPluginRuntime::new()?;
    runtime.load_wit_plugin(name, &wasm_bytes).await?;

    let (db_cb, rid) = (db.clone(), run_id);
    let on_result = Arc::new(move |_id: u64, data: Vec<u8>| {
        let db = db_cb.clone();
        let data = data.clone();
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
            .bind(rid).bind(seq).bind(&data)
            .execute(&db).await;

            let _ = sqlx::query(
                "UPDATE workflow_runs SET state = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(&data).bind(rid)
            .execute(&db).await;
        });
    });

    let params = serde_json::json!({ "workflow_name": name, "input": input_str });
    let result = runtime
        .execute_wit_function_with_hook(name, "start-workflow", params, Some(on_result))
        .await;

    finish_run(&db, run_id, result, args.pretty).await
}

async fn finish_run(
    db: &SqlitePool,
    run_id: i64,
    result: Result<serde_json::Value>,
    pretty: bool,
) -> Result<()> {
    match result {
        Ok(output) => {
            let output_str = output.to_string();
            sqlx::query(
                "UPDATE workflow_runs SET output = ?, status = 'completed', updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(&output_str)
            .bind(run_id)
            .execute(db)
            .await?;

            println!("  {} workflow completed (run #{})", "✓".green().bold(), run_id);
            println!();
            println!("{}", if pretty { serde_json::to_string_pretty(&output)? } else { output_str });
            Ok(())
        }
        Err(e) => {
            sqlx::query(
                "UPDATE workflow_runs SET output = ?, status = 'failed', updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(e.to_string())
            .bind(run_id)
            .execute(db)
            .await?;

            Err(e)
        }
    }
}

// ── Remote: HTTP to running server ───────────────────────────────────────────

async fn run_remote(args: RunArgs) -> Result<()> {
    let server = args.server.as_deref().unwrap();
    let client = reqwest::Client::new();
    let name = args.workflow.as_deref().unwrap();

    let (url, body) = if let Some(run_id) = args.continue_id {
        let state = args.input.as_deref().unwrap_or("{}");
        let url = format!("{}/api/workflows/{}/continue-workflow", server.trim_end_matches('/'), name);
        let body = serde_json::json!({ "run_id": run_id, "state": state });
        (url, body)
    } else {
        let input = args.input.as_deref().unwrap_or("{}");
        serde_json::from_str::<serde_json::Value>(input)
            .with_context(|| format!("Invalid --input JSON: {}", input))?;
        let url = format!("{}/api/workflows/{}/start-workflow", server.trim_end_matches('/'), name);
        let body = serde_json::json!({ "workflow_name": name, "input": input });
        (url, body)
    };

    println!("  {} {} {}", "→".cyan(), "POST".dimmed(), url.dimmed());

    let mut req = client.post(&url).json(&body);
    if let Some(token) = &args.token {
        req = req.bearer_auth(token);
    }

    let response = req
        .send()
        .await
        .with_context(|| format!("Cannot reach '{}'. Is the server running?", server))?;

    let status = response.status();
    let json: serde_json::Value = response.json().await.context("Response is not JSON")?;

    if status.is_success() {
        println!("  {} {}", "✓".green().bold(), status);
    } else {
        println!("  {} {}", "✗".red().bold(), status);
    }
    println!();

    let output = if args.pretty {
        serde_json::to_string_pretty(&json)?
    } else {
        json.to_string()
    };
    println!("{}", output);
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn pick_plugin(plugins_dir: &PathBuf) -> Result<String> {
    let mut names: Vec<String> = vec![];

    if plugins_dir.exists() {
        for entry in std::fs::read_dir(plugins_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }

    if names.is_empty() {
        bail!(
            "No .wasm plugins found in '{}'\nRun: xarxa plugin build --out {}",
            plugins_dir.display(),
            plugins_dir.display(),
        );
    }

    if names.len() == 1 {
        println!("  {} using '{}'", "→".cyan(), names[0]);
        return Ok(names[0].clone());
    }

    println!("  {} Available plugins:", "?".cyan().bold());
    for (i, name) in names.iter().enumerate() {
        println!("    {}  {}", format!("[{}]", i + 1).dimmed(), name);
    }
    print!("\n  Select [1-{}]: ", names.len());

    use std::io::{self, BufRead, Write};
    io::stdout().flush()?;
    let line = io::stdin().lock().lines().next().context("No input")??;
    let idx: usize = line.trim().parse()
        .with_context(|| format!("Expected a number, got '{}'", line.trim()))?;

    if idx < 1 || idx > names.len() {
        bail!("Invalid selection: {}", idx);
    }
    Ok(names[idx - 1].clone())
}

fn prompt_input() -> Result<String> {
    use std::io::{self, BufRead, Write};

    print!("  {} input (JSON, or Enter for empty): ", "?".cyan().bold());
    io::stdout().flush()?;
    let line = io::stdin().lock().lines().next().context("No input")??;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Ok("{}".to_string());
    }

    serde_json::from_str::<serde_json::Value>(trimmed)
        .with_context(|| format!("Invalid JSON: {}", trimmed))?;

    Ok(trimmed.to_string())
}
