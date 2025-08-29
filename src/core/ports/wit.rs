use wasmtime::component;

component::bindgen!({
    path: "wit/engine",
    world: "controller",
    // async: true,
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