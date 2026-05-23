//! `/api/libraries` HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
}

pub async fn list(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, ApiError> {
    let libs = state.libraries.list().await.map_err(ApiError::Internal)?;
    Ok(Json(libs))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateLibraryRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let info = state
        .libraries
        .create(&req.name)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .libraries
        .delete(&name)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn switch(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .libraries
        .set_active(&name)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "active": name })))
}

pub async fn active(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({ "active": state.libraries.active().await }))
}
