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

use uuid::Uuid;
use serde::{Deserialize, Serialize};
use validator::Validate;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, sqlx::Type)]
#[sqlx(type_name = "namespace_role", rename_all = "lowercase")]
pub enum NamespaceRole {
    Owner,
    Admin,
    Editor,
    Viewer,
}

#[derive(Debug, Deserialize, Validate)]
pub struct NewNamespace {
    #[validate(length(min = 5))]
    pub slug: String,
}

#[derive(Debug, Serialize, Validate)]
pub struct Namespace {
    pub id: Uuid,
    pub slug: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct NamespaceMember {
    pub namespace_id: Uuid,
    pub user_id: Uuid,
    pub role: NamespaceRole,
    pub joined_at: DateTime<Utc>,
}