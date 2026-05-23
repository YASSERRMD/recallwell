//! Top-level route table for the recallwell server.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::server::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/config", get(api_config))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>recallwell</title></head>\
         <body><h1>recallwell</h1><p>UI not yet wired.</p></body></html>",
    )
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
