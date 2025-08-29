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
use anyhow::Result;
use axum::{
    Json as JsonResponse,
    Extension,
    response::IntoResponse,
    extract::Json,
};
use validator::Validate;

use super::ApiError;
use crate::{
    adapters::http::auth::Claims, 
    core::{
        domain::namespace::{NewNamespace}, 
        services::namespace::NamespaceService,
    },
};

pub(super) async fn create_namespace(
    claims: Claims,
    Extension(namespace_service): Extension<Arc<NamespaceService>>,
    Json(req): Json<NewNamespace>,
) -> Result<impl IntoResponse, ApiError> {
    req.validate()?;
    
    let u = namespace_service.create(claims.get_user_id(), &req).await?;

    Ok(JsonResponse(u))
}

pub(super) async fn get_namespaces(
    claims: Claims,
    Extension(namespace_service): Extension<Arc<NamespaceService>>,
) -> Result<impl IntoResponse, ApiError> {
    
    let u = namespace_service.find_all(claims.get_user_id()).await?;

    Ok(JsonResponse(u))
}