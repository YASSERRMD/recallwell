//! Top-level route table for the recallwell server.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete as delete_method, get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::handlers;
use crate::server::AppState;
use crate::ui;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/assets/*path", get(asset))
        .route("/api/health", get(health))
        .route("/api/config", get(api_config))
        .route("/api/stats", get(api_stats))
        .route(
            "/api/libraries",
            get(handlers::libraries::list).post(handlers::libraries::create),
        )
        .route("/api/libraries/active", get(handlers::libraries::active))
        .route(
            "/api/libraries/:name",
            delete_method(handlers::libraries::delete),
        )
        .route(
            "/api/libraries/:name/switch",
            post(handlers::libraries::switch),
        )
        .route(
            "/api/ingest",
            get(handlers::ingest::list).post(handlers::ingest::upload),
        )
        .route("/api/ingest/:id", get(handlers::ingest::status))
        .route("/api/ingest/:id/stream", get(handlers::ingest::stream))
        .route("/api/ask", post(handlers::ask::ask))
        .route("/api/source/:library/:doc_id", get(handlers::source::open))
        .route(
            "/api/history",
            get(handlers::history::list).delete(handlers::history::clear),
        )
        .route("/api/history/search", get(handlers::history::search))
        .route(
            "/api/history/:id",
            get(handlers::history::get).delete(handlers::history::delete),
        )
        .route("/api/history/:id/export", get(handlers::history::export))
        .with_state(state)
}

async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    let active = state.libraries.active().await;
    Html(ui::render_index(&state.token, &active))
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

#[allow(clippy::cast_possible_wrap)]
async fn api_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime_secs = state.started_at.elapsed().as_secs();
    let libraries = state.libraries.list().await.unwrap_or_default();
    let active = state.libraries.active().await;
    let total_size: u64 = libraries.iter().map(|l| l.file_size_bytes).sum();
    let recent = state.history.list(None, 20, 0).await.unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0);
    let today_start = now - (now % 86_400);
    let asks_today = recent
        .iter()
        .filter(|r| r.created_at >= today_start)
        .count();
    Json(json!({
        "uptime_secs": uptime_secs,
        "library_count": libraries.len(),
        "active_library": active,
        "total_size_bytes": total_size,
        "asks_today": asks_today,
        "synthesis_model": state.config.groq.synthesis_model,
        "navigation_model": state.config.groq.navigation_model,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
