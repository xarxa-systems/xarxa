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

use anyhow::Ok;
use async_trait::async_trait;
use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::domain::workflow::{NewWorkflow, Workflow};
use super::workflow_dto::{
    Workflow as WorkflowDTO,
    WorkflowVersion as WorkflowVersionDTO,
};
use crate::core::ports::storage::WorkflowRepository;


pub struct PostgresWorkflowRepository {
    pool: Arc<PgPool>,
}

impl PostgresWorkflowRepository {
    pub fn new(pool: Arc<PgPool>) -> impl WorkflowRepository {
        PostgresWorkflowRepository{
            pool,
        }
    }
}

const DEFAULT_CHANGELOG: &str = "Init";
const PLACEHOLDER: &str = "0.0.0";

#[async_trait]
impl WorkflowRepository for PostgresWorkflowRepository {
    async fn insert(&self, user_id: Uuid, ns_id: Uuid, w: &NewWorkflow) -> Result<Workflow, anyhow::Error> {
        let (workflow_id, workflow_version_id) = (Uuid::now_v7(), Uuid::now_v7());

        let mut tx = self.pool.begin().await?;

        let wf = sqlx::query_as!(
            WorkflowDTO,
            r#"
            INSERT INTO workflows (id, namespace_id, key, display_name, description ,created_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, namespace_id, key, display_name, description, active_version_id, is_archived, created_by, created_at, updated_at
            "#,
            workflow_id,
            ns_id,
            w.key,
            w.display_name,
            w.description,
            user_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let wfv = sqlx::query_as!(
            WorkflowVersionDTO,
            r#"
            INSERT INTO workflow_versions (id, workflow_id, version, wasm_md5, wasm_size_bytes, storage_url, created_by, changelog)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, workflow_id, version, wasm_md5, wasm_size_bytes, storage_url, created_by, changelog
            "#,
            workflow_version_id,
            workflow_id,
            PLACEHOLDER,
            w.wasm_md5.as_slice(),
            w.wasm_size_bytes as i64,
            w.storage_url,
            user_id,
            DEFAULT_CHANGELOG
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            UPDATE workflows SET active_version_id = $1 WHERE id = $2
            "#,
            workflow_version_id,
            workflow_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(wf.to_domain(wfv.version))
    }

    // async fn find_by_id(&self, id: Uuid) -> Result<Option<Workflow>, anyhow::Error> {
    //     let wf = sqlx::query_as!(
    //         WorkflowDTO,
    //         r#"
    //         SELECT id, namespace_id, key, display_name, description, active_version_id, is_archived, created_by, created_at, updated_at
    //         FROM workflows
    //         WHERE id = $1
    //         "#,
    //         id
    //     )
    //     .fetch_optional(&*self.pool)
    //     .await?;

    //     Ok(wf.to_domain(wfv.version))
    // }

    //  async fn find_by_key(&self, key: &str) -> Result<Option<WorkflowDTO>, anyhow::Error> {
    //     let wf = sqlx::query_as!(
    //         WorkflowDTO,
    //         r#"
    //         SELECT id, namespace_id, key, display_name, description, active_version_id, is_archived, created_by, created_at, updated_at
    //         FROM workflows
    //         WHERE key = $1
    //         "#,
    //         key
    //     )
    //     .fetch_optional(&*self.pool)
    //     .await?;

    //     Ok(wf)
    // }

    async fn find_all(&self, ns_id: Uuid) -> Result<Vec<Workflow>, anyhow::Error> {
        let items = sqlx::query_as!(
            Workflow,
            r#"
            SELECT w.id, w.namespace_id, w.key, w.display_name, w.description, wv.version as active_version, w.is_archived, w.created_by, w.created_at, w.updated_at 
            FROM workflows w 
                JOIN workflow_versions wv 
                ON w.id = wv.workflow_id
            WHERE w.namespace_id = $1
            ORDER BY w.created_at DESC
            "#,
            ns_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(items)
    }
}