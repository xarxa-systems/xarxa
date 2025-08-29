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