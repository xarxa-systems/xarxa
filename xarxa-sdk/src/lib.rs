/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * Xarxa is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
 * See the LICENSE file in the project root for the full license text.
 *
 * Commercial licensing (MIT / proprietary) is available.
 * Contact: contact@xarxa.io
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

//! # Xarxa SDK
//!
//! Write durable WASM workflows as simple Rust functions.
//!
//! ## Quick start
//!
//! ```ignore
//! use xarxa_sdk::prelude::*;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct BookingInput {
//!     flight_id: String,
//!     user_id: String,
//!     amount: f64,
//! }
//!
//! #[workflow("booking")]
//! fn booking_flow(input: BookingInput) -> PipelineState {
//!     let flight_id = input.flight_id.clone();
//!
//!     Pipeline::new()
//!         .step("reserve", move |_ctx| {
//!             Ok(())
//!         })
//!         .step("charge", move |_ctx| {
//!             Ok(())
//!         })
//!         .build()
//! }
//!
//! xarxa_entry_point!();
//! ```

pub mod bindings;
pub mod testing;
mod pipeline;
mod history;
mod context;
mod registry;

pub use pipeline::{Pipeline, PipelineState, StepResult};
pub use history::WorkflowHistory;
pub use context::StepContext;
pub use registry::WorkflowRegistration;

// Re-exports needed by proc-macros
pub use inventory;
pub use serde_json;

pub use xarxa_sdk_macros::{workflow, xarxa_entry_point};

pub mod prelude {
    pub use crate::{Pipeline, PipelineState, StepContext};
    pub use xarxa_sdk_macros::{workflow, xarxa_entry_point};
    pub use serde::{Deserialize, Serialize};
}
