/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! Local test harness — run workflows in-process without WASM or PostgreSQL.
//!
//! # Example
//!
//! ```ignore
//! use xarxa_sdk::testing::WorkflowTestRunner;
//!
//! #[test]
//! fn test_booking_workflow() {
//!     let input = serde_json::json!({ "flight_id": "AA123", "user_id": "u1", "amount": 99.0 });
//!     let mut runner = WorkflowTestRunner::new("booking", input);
//!
//!     // Run to completion (all steps)
//!     let outcome = runner.run_to_completion();
//!     assert!(outcome.is_done);
//!     assert!(outcome.error.is_none());
//!     assert_eq!(outcome.completed_steps, vec!["reserve", "charge"]);
//! }
//! ```

use crate::{
    history::WorkflowHistory,
    pipeline::StepResult,
    registry::WorkflowRegistration,
};

/// Result of running a workflow through the test harness.
#[derive(Debug)]
pub struct TestOutcome {
    pub completed_steps: Vec<String>,
    pub is_done: bool,
    pub error: Option<String>,
}

/// In-process workflow runner for unit/integration tests.
pub struct WorkflowTestRunner {
    workflow_name: String,
    raw_input: Vec<u8>,
    history: Option<WorkflowHistory>,
}

impl WorkflowTestRunner {
    /// Create a new runner for the named workflow with JSON input.
    pub fn new(workflow_name: impl Into<String>, input: serde_json::Value) -> Self {
        WorkflowTestRunner {
            workflow_name: workflow_name.into(),
            raw_input: serde_json::to_vec(&input).expect("input must be serializable"),
            history: None,
        }
    }

    /// Execute all pending steps to completion.
    pub fn run_to_completion(&mut self) -> TestOutcome {
        let registration = self.find_registration();

        if self.history.is_none() {
            // First call — equivalent to start_workflow
            let mut pipeline = (registration.start)(&self.raw_input);
            let (completed, final_result) = pipeline.run_to_completion();
            let history = WorkflowHistory::new_with_input(
                &self.workflow_name,
                self.raw_input.clone(),
                &pipeline,
            );
            self.history = Some(history);
            return TestOutcome {
                completed_steps: completed,
                is_done: final_result.is_done,
                error: final_result.error,
            };
        }

        // Subsequent calls — equivalent to continue_workflow
        let history = self.history.as_ref().unwrap().clone();
        let mut pipeline = (registration.start)(&history.serialized_input);
        pipeline.replay_from_history(&history.completed_steps);
        let (completed, final_result) = pipeline.run_to_completion();
        let updated = history.advance(final_result.completed_step.clone(), final_result.is_done);
        self.history = Some(updated);

        TestOutcome {
            completed_steps: completed,
            is_done: final_result.is_done,
            error: final_result.error,
        }
    }

    /// Advance a single step (useful for testing step-by-step behaviour).
    pub fn tick(&mut self) -> StepResult {
        let registration = self.find_registration();

        if self.history.is_none() {
            let mut pipeline = (registration.start)(&self.raw_input);
            let result = pipeline.tick();
            let history = WorkflowHistory::new_with_input(
                &self.workflow_name,
                self.raw_input.clone(),
                &pipeline,
            );
            self.history = Some(history);
            return result;
        }

        let history = self.history.as_ref().unwrap().clone();
        let mut pipeline = (registration.start)(&history.serialized_input);
        pipeline.replay_from_history(&history.completed_steps);
        let result = pipeline.tick();
        let updated = history.advance(result.completed_step.clone(), result.is_done);
        self.history = Some(updated);
        result
    }

    fn find_registration(&self) -> &'static WorkflowRegistration {
        inventory::iter::<WorkflowRegistration>
            .into_iter()
            .find(|r| r.name == self.workflow_name.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "Workflow '{}' not registered. Did you add #[workflow(\"{}\")]?",
                    self.workflow_name, self.workflow_name
                )
            })
    }
}
