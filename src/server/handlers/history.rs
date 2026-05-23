//! `/api/history` HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub library: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

const fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ClearQuery {
    #[serde(default)]
    pub library: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let rows = state
        .history
        .list(q.library.as_deref(), q.limit, q.offset)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(rows))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let row = state
        .history
        .get(&id)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("history {id}")))?;
    Ok(Json(row))
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if q.q.trim().is_empty() {
        return Err(ApiError::BadRequest("q is empty".into()));
    }
    let rows = state
        .history
        .search(&q.q, q.limit)
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(rows))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .history
        .delete(&id)
        .await
        .map_err(ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ClearQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let n = state
        .history
        .clear(q.library.as_deref())
        .await
        .map_err(ApiError::Internal)?;
    Ok(Json(json!({ "cleared": n })))
}
