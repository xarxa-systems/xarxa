mod workflow_handler;
mod user_handler;
mod namespace_handler;
mod auth;

use axum::{
    Extension,
    routing::{get, post, delete},
    http::StatusCode,
    response::Json,
    response::{IntoResponse, Response},
    Router,
    extract::{DefaultBodyLimit},
};
use tower_http::trace::TraceLayer;
use aws_sdk_s3::client::Client as s3c;
use serde_json::json;
use tokio::sync::oneshot;
use tracing::info;
use std::sync::Arc;
use anyhow::{Result as AnyhowResult};

use crate::{
    adapters::wasmtime::wit_runtime::WitPluginRuntime, 
    core::services::{namespace::NamespaceService, user::UserService, workflow::WorkflowService}, 
    infra::config::AppConfig,
};

use super::http::{
    workflow_handler::{run_workflow, remove_plugin_endpoint, create_workflow, get_workflows},
    user_handler::{signup, signin},
    namespace_handler::{create_namespace, get_namespaces},
};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub details: Option<String>,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            details: None,
        }
    }
    
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            details: None,
        }
    }

        pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        let error_chain: Vec<String> = err.chain()
            .map(|e| e.to_string())
            .collect();
            
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
            details: if error_chain.len() > 1 {
                Some(error_chain[1..].join(" -> "))
            } else {
                None
            },
        }
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(err: validator::ValidationErrors) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: err.to_string(),
            details: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({
            "success": false,
            "error": self.message
        });
        
        if let Some(details) = self.details {
            body["details"] = json!(details);
        }
        
        (self.status, Json(body)).into_response()
    }
}

pub async fn start_server(
    s3_client: Arc<s3c>,
    workflows_service: Arc<WorkflowService>,
    user_service: Arc<UserService>,
    namespace_service: Arc<NamespaceService>,
    shutdown_rx: oneshot::Receiver<()>,
    wit_runtime: Arc<WitPluginRuntime>,
    cfg: Arc<AppConfig>,
) -> AnyhowResult<()> {
    info!("Starting HTTP server on port {:?}", cfg.port);

    let app = Router::new()
        // .route("/workflows", get(workflows_list))
        // .route("/workflows/{workflow_key}/functions", get(list_functions))
        .route("/auth/signup", post(signup))
        .route("/auth/signin", post(signin))

        .route("/namespaces", post(create_namespace))
        .route("/namespaces", get(get_namespaces))

        .route("/namespaces/{id}/workflows", post(create_workflow))
        .route("/namespaces/{id}/workflows", get(get_workflows))

        .route("/workflows/{workflow_key}/{function}", post(run_workflow))
        .route("/workflows/{workflow_key}", delete(remove_plugin_endpoint))

        .route("/health", get(health_check))
        .layer(Extension(wit_runtime))
        .layer(Extension(s3_client))
        .layer(Extension(workflows_service))
        .layer(Extension(user_service))
        .layer(Extension(namespace_service))
        .layer(DefaultBodyLimit::max(30485760)) // ~30mb
        .layer(TraceLayer::new_for_http());

    let app = Router::new().nest("/api", app);

    let addr = format!("0.0.0.0:{}", cfg.port);

    let listener = tokio::net::TcpListener::bind(addr.clone()).await?;
    info!("HTTP server listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            
            info!("🛑 Received shutdown signal for HTTP server");
        })
        .await?;

    info!("HTTP server shutdown complete");
    Ok(())
}

async fn health_check() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy",
            "service": "http_server"
        })),
    )
}