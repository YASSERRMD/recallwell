//! `/api/ask` HTTP handler: stream a pagebridge answer as SSE.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::response::sse::Event;
use axum::response::IntoResponse;
use axum::Json;
use futures::Stream;
use futures::StreamExt;
use pagebridge::AnswerChunk;
use serde::Deserialize;
use serde_json::json;

use crate::server::error::ApiError;
use crate::server::sse;
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub question: String,
    #[serde(default)]
    pub library: Option<String>,
}

pub async fn ask(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AskRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let question = req.question.trim();
    if question.is_empty() {
        return Err(ApiError::BadRequest("question is empty".into()));
    }
    if question.len() > 4_000 {
        return Err(ApiError::BadRequest(
            "question too long (max 4000 chars)".into(),
        ));
    }

    let library = match req.library {
        Some(name) if !name.is_empty() => name,
        _ => state.libraries.active().await,
    };
    let bridge = state
        .libraries
        .open(&library)
        .await
        .map_err(ApiError::Internal)?;

    let question = question.to_string();
    let stream = bridge
        .ask_stream(&question)
        .await
        .map_err(|e| ApiError::Pagebridge(e.to_string()))?;

    let sse_stream = chunk_stream_to_sse(stream);
    Ok(sse::sse_response(sse_stream))
}

fn chunk_stream_to_sse<S>(stream: S) -> impl Stream<Item = Result<Event, Infallible>>
where
    S: Stream<Item = pagebridge::Result<AnswerChunk>> + Send + 'static,
{
    async_stream::stream! {
        let mut s = Box::pin(stream);
        while let Some(item) = s.next().await {
            match item {
                Ok(AnswerChunk::Token { text }) => {
                    if let Ok(ev) = sse::event_json("token", &json!({ "text": text })) {
                        yield Ok::<_, Infallible>(ev);
                    }
                }
                Ok(AnswerChunk::Citation { citation }) => {
                    let payload = json!({
                        "node_id": citation.node_id.to_string(),
                        "doc_id": citation.doc_id.to_string(),
                        "doc_title": citation.doc_title,
                        "section_title": citation.section_title,
                        "page_range": citation.page_range,
                        "excerpt": citation.excerpt,
                    });
                    if let Ok(ev) = sse::event_json("citation", &payload) {
                        yield Ok::<_, Infallible>(ev);
                    }
                }
                Ok(AnswerChunk::Done { trace, citations }) => {
                    let payload = json!({
                        "query_id": trace.query_id,
                        "duration_ms": trace.duration_ms,
                        "total_input_tokens": trace.total_input_tokens,
                        "total_output_tokens": trace.total_output_tokens,
                        "citation_count": citations.len(),
                    });
                    if let Ok(ev) = sse::event_json("done", &payload) {
                        yield Ok::<_, Infallible>(ev);
                    }
                    break;
                }
                Err(e) => {
                    let payload = json!({ "error": e.to_string() });
                    if let Ok(ev) = sse::event_json("error", &payload) {
                        yield Ok::<_, Infallible>(ev);
                    }
                    break;
                }
            }
        }
    }
}
