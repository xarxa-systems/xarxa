/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! `Pipeline` — the core workflow building block.
//!
//! A workflow is described as an ordered list of steps with optional
//! compensations (for Saga rollback).  The runtime calls `tick()` to
//! advance the pipeline one step at a time, persisting history between calls.
//!
//! # Example
//!
//! ```ignore
//! Pipeline::new()
//!     .step("reserve", |ctx| {
//!         // ... do work ...
//!         Ok(())
//!     })
//!     .compensate("reserve", |ctx| {
//!         // ... roll back ...
//!         Ok(())
//!     })
//!     .step("charge", |ctx| Ok(()))
//!     .build()
//! ```

use crate::context::StepContext;

type StepFn = Box<dyn FnOnce(&mut StepContext) -> Result<(), String>>;

pub(crate) struct Step {
    name: String,
    action: StepFn,
    compensation: Option<StepFn>,
}

/// Builder for constructing a pipeline.
pub struct Pipeline {
    steps: Vec<Step>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }

    /// Add an executable step to the pipeline.
    pub fn step<F>(mut self, name: impl Into<String>, f: F) -> Self
    where
        F: FnOnce(&mut StepContext) -> Result<(), String> + 'static,
    {
        self.steps.push(Step {
            name: name.into(),
            action: Box::new(f),
            compensation: None,
        });
        self
    }

    /// Attach a compensation (rollback) handler to the most recently added step.
    ///
    /// # Panics
    /// Panics if called before any step has been added.
    pub fn compensate<F>(mut self, name: impl Into<String>, f: F) -> Self
    where
        F: FnOnce(&mut StepContext) -> Result<(), String> + 'static,
    {
        let name = name.into();
        let step = self
            .steps
            .iter_mut()
            .rev()
            .find(|s| s.name == name)
            .expect("compensate() called for a step name that does not exist");
        step.compensation = Some(Box::new(f));
        self
    }

    /// Finalise the builder into a `PipelineState` ready for execution.
    pub fn build(self) -> PipelineState {
        PipelineState::new(self.steps)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ── PipelineState ─────────────────────────────────────────────────────────────

/// The executable state of a pipeline.
///
/// Returned by `Pipeline::build()` and also reconstructed by the runtime
/// when replaying from persisted history.
pub struct PipelineState {
    steps: Vec<Step>,
    /// Index of the next step to execute.
    cursor: usize,
    /// Names of steps that have already succeeded (used for replay).
    completed: Vec<String>,
    /// Whether the pipeline has encountered a failure requiring compensation.
    failed: bool,
}

impl PipelineState {
    pub(crate) fn new(steps: Vec<Step>) -> Self {
        PipelineState {
            steps,
            cursor: 0,
            completed: Vec::new(),
            failed: false,
        }
    }

    /// Reconstruct cursor state from previously persisted completed step names.
    ///
    /// Called by `xarxa_entry_point!()` during `continue_workflow` replay.
    pub fn replay_from_history(&mut self, completed_steps: &[String]) {
        for name in completed_steps {
            if let Some(pos) = self.steps.iter().position(|s| &s.name == name) {
                if pos >= self.cursor {
                    self.cursor = pos + 1;
                }
                self.completed.push(name.clone());
            }
        }
    }

    /// Convert into a `Pipeline` (for macro-generated code).
    pub fn into_pipeline(self) -> Self {
        self
    }

    /// Execute the next pending step.
    ///
    /// Returns a `StepResult` describing what happened.
    pub fn tick(&mut self) -> StepResult {
        if self.cursor >= self.steps.len() {
            return StepResult {
                completed_step: None,
                is_done: true,
                error: None,
            };
        }

        // Safety: cursor is bounds-checked above; we move out of the Vec
        // by swapping with a dummy to satisfy the borrow checker.
        let step_idx = self.cursor;
        let step_name = self.steps[step_idx].name.clone();

        // Take the action out of the step (it's FnOnce)
        let action = {
            // Replace with a no-op so we can take ownership
            let dummy: StepFn = Box::new(|_| Ok(()));
            let action = std::mem::replace(&mut self.steps[step_idx].action, dummy);
            action
        };

        let mut ctx = StepContext::new(&step_name, 0);
        match (action)(&mut ctx) {
            Ok(()) => {
                self.completed.push(step_name.clone());
                self.cursor += 1;
                let is_done = self.cursor >= self.steps.len();
                StepResult {
                    completed_step: Some(step_name),
                    is_done,
                    error: None,
                }
            }
            Err(e) => {
                self.failed = true;
                StepResult {
                    completed_step: None,
                    is_done: false,
                    error: Some(format!("Step '{}' failed: {}", step_name, e)),
                }
            }
        }
    }

    /// Run to completion, executing every pending step.
    ///
    /// Returns the names of all steps completed in this call plus the final
    /// `StepResult`.  Stops on first error.
    pub fn run_to_completion(&mut self) -> (Vec<String>, StepResult) {
        let mut newly_completed = Vec::new();
        loop {
            let result = self.tick();
            if let Some(ref name) = result.completed_step {
                newly_completed.push(name.clone());
            }
            if result.is_done || result.error.is_some() {
                return (newly_completed, result);
            }
        }
    }

    /// Capture the list of completed step names (for persisting to history).
    pub fn capture_history(&self) -> Vec<String> {
        self.completed.clone()
    }
}

// ── StepResult ────────────────────────────────────────────────────────────────

/// Result of a single `tick()` call.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Name of the step that was just completed, if any.
    pub completed_step: Option<String>,
    /// `true` when all steps have been executed successfully.
    pub is_done: bool,
    /// Error message if the step failed.
    pub error: Option<String>,
}
