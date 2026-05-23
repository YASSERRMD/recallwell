//! `/api/history` HTTP handlers.

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
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<axum::response::Response, ApiError> {
    let rows = state
        .history
        .list(q.library.as_deref(), q.limit, q.offset)
        .await
        .map_err(ApiError::Internal)?;

    let wants_html = headers
        .get("hx-request")
        .map(|v| v == HeaderValue::from_static("true"))
        .unwrap_or(false);

    if wants_html {
        if rows.is_empty() {
            return Ok((
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                Html(r#"<div class="text-xs text-zinc-500">No asks yet.</div>"#.to_string()),
            )
                .into_response());
        }
        let mut out = String::new();
        for r in rows {
            let q_short = html_escape(&shorten(&r.question, 80));
            let lib = html_escape(&r.library);
            out.push_str(&format!(
                r#"<div class="text-xs p-2 rounded border border-zinc-200 dark:border-zinc-800">
                       <div class="font-medium" title="{q_full}">{q_short}</div>
                       <div class="text-zinc-500 mt-1">{lib}</div>
                   </div>"#,
                q_full = html_escape(&r.question),
            ));
        }
        Ok((
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            Html(out),
        )
            .into_response())
    } else {
        Ok(Json(rows).into_response())
    }
}

fn shorten(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push_str("...");
        out
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    pub format: Option<String>,
}

pub async fn export(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> Result<axum::response::Response, ApiError> {
    let format = q.format.as_deref().unwrap_or("markdown");
    if format != "markdown" && format != "md" {
        return Err(ApiError::BadRequest(format!(
            "unsupported format: {format} (v0.1 supports markdown only)"
        )));
    }
    let ask = state
        .history
        .get(&id)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::NotFound(format!("history {id}")))?;
    let body = crate::export::answer_to_markdown(&ask);
    let filename = crate::export::filename_for(&ask);
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "text/markdown; charset=utf-8".to_string(),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
    )
        .into_response())
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
