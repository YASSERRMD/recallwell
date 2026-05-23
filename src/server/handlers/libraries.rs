//! `/api/libraries` HTTP handlers.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// If set, render as an HTMX-friendly HTML fragment.
    #[serde(default)]
    pub html: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLibraryRequest {
    pub name: String,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<axum::response::Response, ApiError> {
    let libs = state.libraries.list().await.map_err(ApiError::Internal)?;
    let active = state.libraries.active().await;
    let wants_html = q.html.unwrap_or(0) != 0
        || headers
            .get("hx-request")
            .map(|v| v == HeaderValue::from_static("true"))
            .unwrap_or(false);
    if wants_html {
        let mut items = String::new();
        if libs.is_empty() {
            items.push_str(
                r#"<div class="px-2 py-2 text-xs text-zinc-500">No libraries yet.</div>"#,
            );
        } else {
            for lib in &libs {
                let marker = if lib.name == active { "active" } else { "" };
                items.push_str(&format!(
                    r#"<button class="w-full text-left px-2 py-1.5 text-sm rounded hover:bg-zinc-100 dark:hover:bg-zinc-800 flex items-center justify-between"
                       hx-post="/api/libraries/{name}/switch"
                       hx-swap="none"
                       onclick="setTimeout(() => location.reload(), 100)">
                       <span>{name}</span><span class="text-[10px] text-indigo-500">{marker}</span>
                    </button>"#,
                    name = html_escape(&lib.name),
                    marker = marker,
                ));
            }
        }
        items.push_str(
            r#"<hr class="my-1 border-zinc-200 dark:border-zinc-800">
            <form hx-post="/api/libraries" hx-swap="none"
                  onsubmit="setTimeout(() => location.reload(), 200)"
                  hx-ext="json-enc"
                  class="flex gap-1 p-1">
              <input name="name" placeholder="new library" required pattern="[a-z0-9_-]{1,64}"
                     class="flex-1 px-2 py-1 text-sm rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900">
              <button class="px-2 py-1 text-xs bg-indigo-600 text-white rounded">add</button>
            </form>"#,
        );
        Ok((
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            Html(items),
        )
            .into_response())
    } else {
        Ok(Json(libs).into_response())
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
