/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `xarxa plugin` — manage workflow WASM plugins.
//!
//! ```bash
//! xarxa plugin init my-workflow        # scaffold a new plugin project
//! xarxa plugin build                   # cargo build --target wasm32-wasip2
//! xarxa plugin build --out ./plugins   # build + copy to plugins dir
//! ```

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Scaffold a new workflow plugin project
    Init(InitArgs),
    /// Compile the plugin to a WASM component
    Build(BuildArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Name of the new plugin (also the directory name)
    #[arg(value_name = "NAME")]
    pub name: String,
}

#[derive(Args)]
pub struct BuildArgs {
    /// Copy the compiled .wasm to this directory after building
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,

    /// Build in release mode (default: true)
    #[arg(long, default_value = "true")]
    pub release: bool,
}

pub async fn run(cmd: PluginCommand) -> Result<()> {
    match cmd {
        PluginCommand::Init(args) => init(args),
        PluginCommand::Build(args) => build(args).await,
    }
}

// ── init ──────────────────────────────────────────────────────────────────────

fn init(args: InitArgs) -> Result<()> {
    let root = PathBuf::from(&args.name);
    if root.exists() {
        bail!("'{}' already exists", root.display());
    }

    let src = root.join("src");
    std::fs::create_dir_all(&src)?;

    std::fs::write(root.join("xarxa.toml"), xarxa_toml(&args.name))?;
    std::fs::write(root.join("Cargo.toml"), cargo_toml(&args.name))?;
    std::fs::write(src.join("lib.rs"), lib_rs(&args.name))?;

    println!("{} {}", "✓ Created".green().bold(), root.display());
    println!();
    println!("  Next steps:");
    println!("    {} {}", "cd".cyan(), args.name);
    println!("    {}", "xarxa plugin build --out ../../plugins".cyan());
    println!("    {}", "xarxa server start --dev".cyan());
    Ok(())
}

// ── build ─────────────────────────────────────────────────────────────────────

async fn build(args: BuildArgs) -> Result<()> {
    // Read xarxa.toml to get the project name and wasm output path
    let config_path = PathBuf::from("xarxa.toml");
    let config = read_xarxa_toml(&config_path)?;

    println!("  {} {} ...", "building".dimmed(), config.project.name);

    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args(["build", "--target", "wasm32-wasip2"]);
    if args.release {
        cmd.arg("--release");
    }

    let status = cmd
        .status()
        .await
        .context("Failed to run `cargo build`. Is Rust installed with wasm32-wasip2 target?")?;

    if !status.success() {
        bail!("`cargo build` failed");
    }

    let profile = if args.release { "release" } else { "debug" };
    let wasm_src = PathBuf::from(format!(
        "target/wasm32-wasip2/{}/{}.wasm",
        profile,
        config.project.name.replace('-', "_"),
    ));

    if !wasm_src.exists() {
        bail!("Expected WASM output at '{}' but it wasn't found", wasm_src.display());
    }

    println!("  {} {} ({})", "✓ built".green(), wasm_src.display(),
        format_size(std::fs::metadata(&wasm_src)?.len()));

    // Copy to --out dir if specified
    if let Some(out_dir) = args.out {
        std::fs::create_dir_all(&out_dir)?;
        let dest = out_dir.join(format!("{}.wasm", config.project.name));
        std::fs::copy(&wasm_src, &dest)?;
        println!("  {} {}", "✓ copied".green(), dest.display());
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.0}MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0}KB", bytes as f64 / 1024.0)
    }
}

// ── xarxa.toml parsing ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct XarxaToml {
    project: ProjectSection,
}

#[derive(serde::Deserialize)]
struct ProjectSection {
    name: String,
}

fn read_xarxa_toml(path: &PathBuf) -> Result<XarxaToml> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!(
            "Cannot read '{}'. Are you inside a plugin directory?",
            path.display()
        ))?;
    toml::from_str(&content)
        .with_context(|| format!("Failed to parse '{}'", path.display()))
}

// ── scaffold templates ────────────────────────────────────────────────────────

fn xarxa_toml(name: &str) -> String {
    format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
"#
    )
}

fn cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
xarxa-sdk = {{ version = "0.1" }}
serde = {{ version = "1", features = ["derive"] }}
wit-bindgen = "0.54"
"#
    )
}

fn lib_rs(name: &str) -> String {
    let struct_name = to_pascal_case(name);
    format!(
        r#"use xarxa_sdk::prelude::*;

#[derive(Deserialize)]
struct {struct_name}Input {{
    // TODO: define your workflow input fields here
    message: String,
}}

#[workflow("{name}")]
fn {snake}(input: {struct_name}Input) -> PipelineState {{
    let msg = input.message.clone();

    Pipeline::new()
        .step("step_one", move |_ctx| {{
            Ok(())
        }})
        .step("step_two", move |_ctx| {{
            Ok(())
        }})
        .build()
}}

xarxa_entry_point!();
"#,
        struct_name = struct_name,
        name = name,
        snake = name.replace('-', "_"),
    )
}

fn to_pascal_case(s: &str) -> String {
    s.split(['-', '_'])
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}
