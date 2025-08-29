use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::core::domain::workflow::Workflow as WorkflowDomain;

pub(super) struct Workflow {
    pub id: Uuid,
    pub namespace_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub active_version_id: Option<Uuid>,
    pub is_archived: bool,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[allow(dead_code)]
pub(super) struct WorkflowVersion {
    pub id: Uuid, 
    pub workflow_id: Uuid, 
    pub version: String, 
    pub wasm_md5: Vec<u8>, 
    pub wasm_size_bytes: i64, 
    pub storage_url: Option<String>, 
    pub created_by: Uuid, 
    pub changelog: Option<String>,
}

impl Workflow {
    pub(super) fn to_domain(self, version: String) -> WorkflowDomain {
        WorkflowDomain {
            id: self.id,
            namespace_id: self.namespace_id,
            key: self.key,
            display_name: self.display_name,
            description: self.description,
            active_version: version,
            is_archived: self.is_archived,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}