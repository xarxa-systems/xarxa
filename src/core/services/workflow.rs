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

use std::sync::Arc;
use anyhow::{ensure};
use aws_sdk_s3::client::Client as s3c;
use uuid::Uuid;

use crate::core::ports::storage::WorkflowRepository;
use crate::core::domain::workflow::{NewWorkflowParams, Workflow, NewWorkflow};
use crate::adapters::wasmtime::wit_runtime::WitPluginRuntime;

pub struct WorkflowService{
    repo: Arc<dyn WorkflowRepository>,
    s3_client: Arc<s3c>,
    wit_runtime: Arc<WitPluginRuntime>,
}

impl WorkflowService {
    #[cold]
    pub fn new(repo: Arc<dyn WorkflowRepository>, s3_client: Arc<s3c>, wit_runtime: Arc<WitPluginRuntime>) -> Self {
        WorkflowService{
            repo,
            s3_client,
            wit_runtime,
        }
    }

    pub async fn create(&self, user_id: Uuid, namespace_id: Uuid, wp: NewWorkflowParams, wasm_bytes: &[u8]) -> Result<Workflow, anyhow::Error> {
        if wasm_bytes.len() >= 4 {
            let magic = &wasm_bytes[0..4];
            ensure!(magic == b"\0asm", "Invalid WASM file format");
        }
        
        let hash = md5::compute(&wasm_bytes);

        let w = NewWorkflow{
            key: wp.key.clone(),
            display_name: wp.display_name.clone(),
            description: wp.description.clone(),
            wasm_md5: hash,
            wasm_size_bytes: wasm_bytes.len(),
            storage_url: "".to_string(),
        };

        // 1. create a new workflow + workflow version
        let db_result = self.repo.insert(user_id,namespace_id, &w).await?;

        let key = format!(
            "{}.wasm", w.key,
        );

        let bucket = "xarxa-s3".to_string();

        // 2. save to s3
        self.s3_client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(wasm_bytes.to_vec().into())
            .acl(aws_sdk_s3::types::ObjectCannedAcl::BucketOwnerFullControl)
            .send()
            .await?;

            // .map_err(|err| Error::InternalServerError(err.to_string()))?;

        // 3. add to runtime
        self.wit_runtime.load_wit_plugin(&w.key, wasm_bytes).await?;


        let resp = Workflow { 
            id: db_result.id,
            namespace_id: db_result.namespace_id, 
            key: db_result.key, 
            display_name: db_result.display_name, 
            description: db_result.description, 
            active_version: "".to_string(), 
            is_archived: db_result.is_archived, 
            created_by: db_result.created_by, 
            created_at: db_result.created_at, 
            updated_at: db_result.updated_at,
        };

        Ok(resp)
    }

    pub async fn update(&self) -> Result<Workflow, anyhow::Error> {
        todo!();
    }
    

    // async fn get_by_key(&self, key: &str) -> Result<Option<Workflow>, anyhow::Error> {
    //     self.repo.find_by_key(key).await
    // }

    pub async fn find_all(&self, ns_id: Uuid) -> Result<Vec<Workflow>, anyhow::Error> {
        self.repo.find_all(ns_id).await
    }
}