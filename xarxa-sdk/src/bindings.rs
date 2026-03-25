/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! Shared types that mirror the WIT definitions.
//! These are used by the SDK runtime (Pipeline, history, etc.)
//! and re-exported for user code.
//!
//! The actual WIT bindgen expansion happens in each user's crate
//! via the `xarxa_entry_point!()` macro.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub input: Vec<KvPair>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowError {
    pub err: String,
    pub code: Code,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Code {
    NotFound,
    EmptyInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvPair {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    None,
    Str(String),
    Int64(i64),
    Uint64(u64),
    Float64(f64),
    Boolean(bool),
    Any(Vec<u8>),
}

/// Log a message via the host logger.
/// Works in both native (prints to stderr) and WASM (calls host import).
pub fn logging(msg: &[u8]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        eprintln!("[xarxa] {}", String::from_utf8_lossy(msg));
    }
    // In WASM the real call is wired by xarxa_entry_point!() via wit-bindgen
    #[cfg(target_arch = "wasm32")]
    {
        let _ = msg; // handled by generated bindings
    }
}
