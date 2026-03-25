/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `StepContext` — passed to each step closure.
//!
//! Currently a lightweight placeholder.  Future versions will add:
//! - `ctx.run_activity(name, input)` — cross-WASM activity calls
//! - `ctx.sleep(duration)` — durable sleep
//! - `ctx.wait_for_signal(name)` — external signal support

/// Context available inside a workflow step.
#[derive(Default)]
pub struct StepContext {
    /// Name of the current step (set by the runtime before calling the closure).
    pub step_name: String,
    /// Run ID assigned by the host when the workflow was started.
    pub run_id: u64,
}

impl StepContext {
    pub(crate) fn new(step_name: impl Into<String>, run_id: u64) -> Self {
        StepContext {
            step_name: step_name.into(),
            run_id,
        }
    }
}
