//! `/api/ingest` HTTP handlers: upload, status, SSE progress stream.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::sse::Event;
use axum::response::IntoResponse;
use axum::Json;
use futures::Stream;
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::ingest::queue::{IngestQueue, IngestState};
use crate::server::error::ApiError;
use crate::server::sse;
use crate::server::AppState;

pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let active_library = state.libraries.active().await;
    let ingested_root = state
        .config
        .ingested_files_dir()
        .map_err(ApiError::Internal)?
        .join(&active_library);
    tokio::fs::create_dir_all(&ingested_root)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut accepted = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        let filename = field
            .file_name()
            .map(str::to_owned)
            .ok_or_else(|| ApiError::BadRequest("file upload requires a filename".into()))?;
        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        if bytes.is_empty() {
            continue;
        }
        let safe_name = sanitize_filename(&filename);
        let job_dir = ingested_root.join(ulid::Ulid::new().to_string());
        tokio::fs::create_dir_all(&job_dir)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
        let file_path = job_dir.join(&safe_name);
        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

        let id = state
            .ingest
            .submit(&active_library, file_path, &safe_name)
            .await
            .map_err(ApiError::Internal)?;
        accepted.push(json!({ "id": id.to_string(), "filename": safe_name }));
    }

    if accepted.is_empty() {
        return Err(ApiError::BadRequest("no files received".into()));
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobs": accepted }))))
}

pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let job = state
        .ingest
        .status(&id)
        .ok_or_else(|| ApiError::NotFound(format!("job {id}")))?;
    Ok(Json(job))
}

pub async fn list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.ingest.list())
}

pub async fn stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let rx = state
        .ingest
        .subscribe(&id)
        .ok_or_else(|| ApiError::NotFound(format!("job {id}")))?;
    let initial = state.ingest.status(&id);

    let head = futures::stream::iter(initial.into_iter().filter_map(|job| {
        let payload = serde_json::to_value(&job.state).ok()?;
        Some(sse::event_json("state", &payload))
    }));

    let updates = BroadcastStream::new(rx).filter_map(|res| {
        let state = res.ok()?;
        let payload = serde_json::to_value(&state).ok()?;
        Some(sse::event_json("state", &payload))
    });

    // After we see a terminal state, close.
    let stream = head.chain(updates).take_while(|res| {
        if let Ok(ev) = res {
            // Stop after a Done or Failed event.
            let data = ev.clone();
            let raw = format!("{data:?}");
            !(raw.contains("\\\"done\\\"") || raw.contains("\\\"failed\\\""))
        } else {
            true
        }
    });

    Ok(sse::sse_response(stream))
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "upload".into()
    } else {
        trimmed.to_string()
    }
}

#[allow(dead_code)]
fn _force_use() -> Option<IngestState> {
    None
}
#[allow(dead_code)]
fn _q(_q: &IngestQueue) {}
