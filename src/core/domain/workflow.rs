use md5::Digest;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize)]
pub struct Workflow {
    pub id: Uuid,
    pub namespace_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub active_version: String,
    pub is_archived: bool,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewWorkflowParams {
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResponse {
    pub success: bool,
    pub result: Option<JsonValue>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct NewWorkflow {
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub wasm_md5: Digest,
    pub wasm_size_bytes: usize,
    pub storage_url: String,
}