//! `/api/source/:library/:doc_id` HTTP handler.
//!
//! Serves the original file that produced the cited passage, so the UI can
//! open it in a new tab (with `#page=N` for PDFs).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use mime_guess::MimeGuess;

use crate::server::error::ApiError;
use crate::server::AppState;
use crate::source;

pub async fn open(
    State(state): State<Arc<AppState>>,
    Path((library, doc_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let entry = source::lookup(&state.config, &library, &doc_id)
        .ok_or_else(|| ApiError::NotFound(format!("source for doc {doc_id}")))?;

    if !source::is_within_data_dir(&entry.file_path, &state.config) {
        return Err(ApiError::BadRequest(
            "source path is outside the data directory".into(),
        ));
    }

    let bytes = tokio::fs::read(&entry.file_path)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    let mime = MimeGuess::from_path(&entry.file_path)
        .first_or_octet_stream()
        .to_string();
    let disposition = format!("inline; filename=\"{}\"", entry.original_filename);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    )
        .into_response())
}
