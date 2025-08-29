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
use uuid::Uuid;

use crate::core::ports::storage::NamespaceRepository;
use crate::core::domain::namespace::{Namespace, NamespaceRole, NewNamespace};

pub struct NamespaceService{
    repo: Arc<dyn NamespaceRepository>,
}

impl NamespaceService {
    #[cold]
    pub fn new(r: Arc<dyn NamespaceRepository>) -> Self {
        NamespaceService{
            repo: r,
        }
    }

    pub async fn create(&self, uid: Uuid, ns: &NewNamespace) -> Result<Namespace, anyhow::Error> {
        self.repo.create(uid, ns).await
    }

    pub async fn find_all(&self, uid: Uuid) -> Result<Vec<Namespace>, anyhow::Error> {
        self.repo.find_by_uid(uid).await
    }

    pub async fn ns_role_by_uid(&self, uid: Uuid, ns_id: Uuid) -> Result<Option<NamespaceRole>, anyhow::Error> {
        self.repo.role_by_uid(uid, ns_id).await
    }
}