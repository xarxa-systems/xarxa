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

use wasmtime::component;

component::bindgen!({
    path: "wit/orchestrator",
    world: "orchestrator",
});



// use async_trait::async_trait;

// use crate::core::domain::wit::{RunId, History};

// #[async_trait]
// pub trait WitRuntime<T> {
//     async fn start_workflow(&self, workflow_key: &str) -> Result<RunId, anyhow::Error>;
//     async fn continue_workflow(&self, history: History<T>) -> Result<(), anyhow::Error>;
//     async fn cancel_workflow_run(&self, run_id: RunId) -> Result<(), anyhow::Error>;
// }

// #[async_trait]
// pub trait HostRuntime {
//     async fn send_result(&self, run_id: RunId) -> Result<(), anyhow::Error>;
// }