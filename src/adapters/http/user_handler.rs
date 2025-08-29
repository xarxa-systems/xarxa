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
use crate::core::services::user::UserService;
use crate::core::domain::user::{NewUser, UserAuth, NewUserResponse};
use super::auth::generate_auth_header;

pub(super) async fn signup(
    Extension(user_service): Extension<Arc<UserService>>,
    Json(req): Json<NewUser>,
) -> Result<impl IntoResponse, ApiError> {
    req.validate()?;
    
    let u = user_service.create(&req).await?;

    Ok(JsonResponse(
        NewUserResponse{
            password: u.token,
        }
    ))
}

pub(super) async fn signin(
    Extension(user_service): Extension<Arc<UserService>>,
    Json(req): Json<UserAuth>,
) -> Result<impl IntoResponse, ApiError> {
    req.validate()?;

    let u = user_service.auth(req.email, req.password).await?;

    let auth_jwt = generate_auth_header(u.clone()).await;

    Ok(auth_jwt)
}