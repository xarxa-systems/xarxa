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
use uuid::Uuid;

use crate::core::domain::{
    user::{NewUser, User},
    workflow::{NewWorkflow, Workflow},
    namespace::{NewNamespace, Namespace, NamespaceRole},
};


#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn insert(&self, user_id: Uuid, ns: Uuid, w: &NewWorkflow) -> Result<Workflow, anyhow::Error>;
    // async fn find_by_id(&self, id: Uuid) -> Result<Option<Workflow>, anyhow::Error>;
    // async fn find_by_key(&self, key: &str) -> Result<Option<Workflow>, anyhow::Error>;
    async fn find_all(&self, ns_id: Uuid) -> Result<Vec<Workflow>, anyhow::Error>;
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, u: &NewUser, token_hash: String) -> Result<User, anyhow::Error>; 
    async fn find_by_email(&self, email: &str) ->  Result<Option<User>, anyhow::Error>;
}

#[async_trait]
pub trait NamespaceRepository: Send + Sync {
    async fn create(&self, uid: Uuid, ns: &NewNamespace) -> Result<Namespace, anyhow::Error>; 
    async fn find_by_uid(&self, uid: Uuid) ->  Result<Vec<Namespace>, anyhow::Error>;
    async fn role_by_uid(&self, uid: Uuid, ns_id: Uuid) ->  Result<Option<NamespaceRole>, anyhow::Error>;
}