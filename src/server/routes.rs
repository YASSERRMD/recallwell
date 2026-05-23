//! Top-level route table for the recallwell server.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::AppState;
use crate::ui;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/:name", get(asset))
        .route("/api/health", get(health))
        .route("/api/config", get(api_config))
        .with_state(state)
}

async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    // Active library name will be filled in by phase 5; for now show a placeholder.
    Html(ui::render_index(&state.token, "default"))
}

async fn asset(Path(name): Path<String>) -> Result<Response, ApiError> {
    let (bytes, ctype) =
        ui::serve_asset(&name).ok_or_else(|| ApiError::NotFound(format!("asset {name}")))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ctype),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    )
        .into_response())
}

async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime_secs = state.started_at.elapsed().as_secs();
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": uptime_secs,
        })),
    )
}

async fn api_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let redacted = state.config.redacted();
    Json(redacted)
}
