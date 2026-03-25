/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `WorkflowHistory` — serializable state passed between `start_workflow`
//! and `continue_workflow` calls.
//!
//! The host persists this as the `state` column in `workflow_events` and
//! passes it back verbatim on every `continue_workflow` invocation.

use serde::{Deserialize, Serialize};
use crate::pipeline::PipelineState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowHistory {
    /// Name of the workflow (used to look up the registered function).
    pub workflow_name: String,
    /// Original serialized input passed to `start_workflow`.
    pub serialized_input: Vec<u8>,
    /// Ordered list of step names that have already completed successfully.
    pub completed_steps: Vec<String>,
    /// `true` once all steps have run to completion.
    pub is_done: bool,
}

impl WorkflowHistory {
    /// Create initial history when a workflow is first started.
    pub fn new(workflow_name: impl Into<String>, pipeline: PipelineState) -> Self {
        WorkflowHistory {
            workflow_name: workflow_name.into(),
            serialized_input: Vec::new(),
            completed_steps: pipeline.capture_history(),
            is_done: false,
        }
    }

    /// Create initial history with the raw input bytes.
    pub fn new_with_input(
        workflow_name: impl Into<String>,
        input: Vec<u8>,
        pipeline: &PipelineState,
    ) -> Self {
        WorkflowHistory {
            workflow_name: workflow_name.into(),
            serialized_input: input,
            completed_steps: pipeline.capture_history(),
            is_done: false,
        }
    }

    /// Return an updated history after a `tick()` advanced the pipeline.
    pub fn advance(mut self, completed_step: Option<String>, is_done: bool) -> Self {
        if let Some(step) = completed_step {
            self.completed_steps.push(step);
        }
        self.is_done = is_done;
        self
    }
}
