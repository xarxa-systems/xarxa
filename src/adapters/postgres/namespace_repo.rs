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

use async_trait::async_trait;
use std::sync::Arc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::core::domain::namespace::{NewNamespace, Namespace, NamespaceRole};
use crate::core::ports::storage::NamespaceRepository;

pub struct PostgresNamespaceRepository {
    pool: Arc<PgPool>,
}

impl PostgresNamespaceRepository {
    pub fn new(pool: Arc<PgPool>) -> impl NamespaceRepository {
        PostgresNamespaceRepository {
            pool,
        }
    }
}

#[async_trait]
impl NamespaceRepository for PostgresNamespaceRepository {
    async fn create(&self, uid: Uuid, ns: &NewNamespace) -> Result<Namespace, anyhow::Error> {
        let namespace_id = Uuid::now_v7();
        
        let mut tx = self.pool.begin().await?;
        
        let namespace = sqlx::query_as!(
            Namespace,
            r#"
            INSERT INTO namespaces (id, slug, created_by)
            VALUES ($1, $2, $3)
            RETURNING id, slug, created_by, created_at
            "#,
            namespace_id,
            ns.slug,
            uid,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO namespace_members (namespace_id, user_id, role)
            VALUES ($1, $2, $3)
            "#,
            namespace_id,
            uid,
            NamespaceRole::Owner as NamespaceRole,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        
        Ok(namespace)
    }

    async fn find_by_uid(&self, uid: Uuid) -> Result<Vec<Namespace>, anyhow::Error> {
        let namespaces = sqlx::query_as!(
            Namespace,
            r#"
            SELECT n.id, n.slug, n.created_by, n.created_at
            FROM namespaces n
            INNER JOIN namespace_members nm ON n.id = nm.namespace_id
            WHERE nm.user_id = $1
            ORDER BY n.created_at DESC
            "#,
            uid
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(namespaces)
    }
    
    async fn role_by_uid(&self, uid: Uuid, ns_id: Uuid) -> Result<Option<NamespaceRole>, anyhow::Error> {
        let role = sqlx::query!(
            r#"
            SELECT role as "role: NamespaceRole"
            FROM namespace_members
            WHERE user_id = $1 AND namespace_id = $2
            "#,
            uid,
            ns_id
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(role.map(|r| r.role))
    }
}