/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! Workflow registry — populated at link time by `#[workflow]` attributes.

use crate::pipeline::PipelineState;

/// A single registered workflow.
///
/// Created by the `#[workflow("name")]` macro via `inventory::submit!`.
pub struct WorkflowRegistration {
    /// The name used to look up this workflow (must match `workflow_name`
    /// argument passed to `start_workflow` / `continue_workflow`).
    pub name: &'static str,
    /// Factory function: deserialises raw input bytes and constructs the
    /// `PipelineState` for this workflow.
    pub start: fn(&[u8]) -> PipelineState,
}

inventory::collect!(WorkflowRegistration);
