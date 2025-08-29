use anyhow::{Result, Context, ensure};
use axum::{
    extract::{Json, Multipart, Path, Query}, 
    response::{IntoResponse, Json as JsonResponse}, 
    Extension
};
use serde_json::{json, Value as JsonValue};
use tracing::info;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    adapters::{
        http::{auth::Claims, ApiError}, 
        wasmtime::wit_runtime::WitPluginRuntime,
    }, 
    core::{domain::workflow::{NewWorkflowParams, Workflow, WorkflowResponse}, services::workflow::WorkflowService},
};


// pub(super) async fn workflows_list(
//     State((workflow_manager, _)): State<(std::sync::Arc<WitPluginManager>, std::sync::Arc<mpsc::UnboundedSender<Message>>)>,
// ) -> (StatusCode, JsonResponse<PluginListResponse>) {
//     let workflows = workflow_manager.list_plugins().await;
//     let response = PluginListResponse { workflows };

//     (StatusCode::OK, JsonResponse(response))
// }

// pub(super) async fn list_functions(
//     State((workflow_manager, _)): State<(std::sync::Arc<WitPluginManager>, std::sync::Arc<mpsc::UnboundedSender<Message>>)>,
//     Path(workflow_key): Path<String>,
// ) -> (StatusCode, JsonResponse<FunctionListResponse>) {
//     match workflow_manager.list_functions(&workflow_key).await {
//         Some(functions) => {
//             let response = FunctionListResponse { workflow: workflow_key, functions };
//             (StatusCode::OK, JsonResponse(response))
//         }
//         None => {
//             let response = FunctionListResponse { workflow: workflow_key, functions: vec![] };
//             (StatusCode::NOT_FOUND, JsonResponse(response))
//         }
//     }
// }

pub(super) async fn run_workflow(
    _claims: Claims,
    wit_runtime: Extension<Arc<WitPluginRuntime>>,
    Path((plugin_name, function_name)): Path<(String, String)>,
    Json(params): Json<serde_json::Value>,
) -> Result<JsonResponse<WorkflowResponse>, ApiError> {
    let result = run_workflow_impl(wit_runtime.0, plugin_name, function_name, params).await?;
    
    Ok(Json(WorkflowResponse {
        success: true,
        result: Some(result),
        error: None,
    }))
}

async fn run_workflow_impl(
    wit_runtime: std::sync::Arc<WitPluginRuntime>,
    plugin_name: String,
    function_name: String,
    params: JsonValue,
) -> Result<JsonValue> {
    ensure!(!plugin_name.is_empty(), "Plugin name cannot be empty");
    ensure!(!function_name.is_empty(), "Function name cannot be empty");
    
    let params_str = params.to_string();
    ensure!(
        params_str.len() <= 1 * 1024 * 1024, // 1MB // TODO: move to config
        "Parameters too large (max 1MB): {} bytes",
        params_str.len()
    );
    
    let result = wit_runtime
        .execute_wit_function(&plugin_name, &function_name, params)
        .await
        .with_context(|| {
            format!("Failed to execute function '{}' in plugin '{}'", function_name, plugin_name)
        })?;
    
    info!("✅ Successfully executed {}.{}", plugin_name, function_name);
    
    Ok(result)
}

pub(super) async fn get_workflows(
    _claims: Claims,
    Path(id): Path<Uuid>,
    Extension(workflow_service): Extension<Arc<WorkflowService>>,
) -> Result<impl IntoResponse, ApiError>  {
    let w = workflow_service.find_all(id).await?;

    Ok(Json(w))
}

pub(super) async fn create_workflow(
    claims: Claims,
    Extension(workflow_service): Extension<Arc<WorkflowService>>,
    params: Query<NewWorkflowParams>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    while let Some(field) = multipart.next_field().await
        .context("Failed to read multipart field")? 
        {
        let name = field.name().unwrap_or("unknown");
        
        if name == "workflow" {
            let filename = field.file_name()
                .ok_or_else(|| ApiError::bad_request("Workflow file must have a filename"))?
                .to_string();

            if !filename.ends_with(".wasm") {
                return Err(ApiError::bad_request(
                    format!("Invalid file extension. Expected .wasm, got: {}", filename)
                ));
            }
            
            let data = field.bytes().await
                .context("Failed to read workflow file data")?;

            if data.is_empty() {
                return Err(ApiError::bad_request(
                    format!("Workflow wasm file is empty: {filename}, len: {}", data.len())
                ));
            }
            
            if data.len() > 50 * 1024 * 1024 { // 50MB
                return Err(ApiError::bad_request(
                    format!("Workflow file too large: {} bytes (max 50MB)", data.len())
                ));
            }

            let workflow_file_name = filename.trim_end_matches(".wasm");

            let w = workflow_service.create(claims.get_user_id(), id,params.0, &data).await?;
            // wit_runtime.load_wit_plugin(workflow_file_name, &data).await?; // todo to service

            // service // todo

            return Ok(Json(json!({
                "success": true,
                "message": format!("Wasm file '{}' with workflow {} uploaded successfully", workflow_file_name, w.key),
                "workflow_name": w.key,
                "file_size": data.len()
            })));
        }
    }
    
    Err(ApiError::bad_request("No plugin file found in request"))
}

pub(super) async fn remove_plugin_endpoint(
    Path(workflow_name): Path<String>,
    wit_runtime: Extension<Arc<WitPluginRuntime>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match wit_runtime.remove_plugin(&workflow_name).await {
        Ok(_) => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Plugin '{}' removed successfully", workflow_name)
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to remove plugin: {}", e)
        })))
    }
}

