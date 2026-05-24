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

use crate::history::{AskRecord, History};
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
    // Pagebridge runs the question through SQLite FTS5 for BM25 candidate
    // selection; FTS5 treats `?`, `*`, `()`, `:`, `^`, `-`, `+`, `"`, etc.
    // as syntax. Strip those before handing the string over so that natural
    // human questions like "what is X about?" don't crash the search.
    let question_for_bridge = sanitize_for_fts5(&question);
    let stream = bridge
        .ask_stream(&question_for_bridge)
        .await
        .map_err(|e| ApiError::Pagebridge(e.to_string()))?;

    let sse_stream = chunk_stream_to_sse(
        stream,
        bridge.clone(),
        state.history.clone(),
        library,
        question,
    );
    Ok(sse::sse_response(sse_stream))
}

/// Replace SQLite FTS5 reserved characters with spaces and collapse
/// whitespace. Keeps alphanumerics and a small set of safe punctuation
/// (underscore, comma, period) that FTS5 tolerates fine.
/// Stateful filter that strips `[[doc:.../leaf:N]]` and any
/// `<citations>...</citations>` block from a streaming token sequence,
/// even when an opening delimiter lands in one chunk and the closing
/// delimiter in another.
#[derive(Debug, Default)]
struct StreamFilter {
    /// Bytes we received but have not yet been able to forward (because they
    /// might start a delimiter whose end is yet to come).
    buf: String,
    /// True once we have seen `<citations>` and are dropping bytes waiting
    /// for `</citations>`.
    in_citations: bool,
    /// True once we have seen `[[` and are dropping bytes waiting for `]]`.
    in_bracket: bool,
    /// True once we have seen `[doc:` (single bracket) and are dropping
    /// bytes waiting for the closing `]`.
    in_single_bracket: bool,
}

impl StreamFilter {
    fn push(&mut self, chunk: &str) -> String {
        self.buf.push_str(chunk);
        let mut emit = String::with_capacity(self.buf.len());

        loop {
            // If we're inside a citations block, drop until </citations>.
            // Keep a tail equal to len(closer)-1 in case the closer is split
            // across chunks.
            if self.in_citations {
                let closer = "</citations>";
                match self.buf.find(closer) {
                    Some(idx) => {
                        let after = idx + closer.len();
                        self.buf = self.buf[after..].to_string();
                        self.in_citations = false;
                    }
                    None => {
                        let keep = closer.len() - 1;
                        if self.buf.len() > keep {
                            let cut = self.buf.len() - keep;
                            // Find a valid char boundary at or before `cut`.
                            let mut k = cut;
                            while k > 0 && !self.buf.is_char_boundary(k) {
                                k -= 1;
                            }
                            self.buf = self.buf[k..].to_string();
                        }
                        break;
                    }
                }
                continue;
            }
            // If we're inside a single-bracket [doc:...] marker, drop until ].
            if self.in_single_bracket {
                match self.buf.find(']') {
                    Some(idx) => {
                        self.buf = self.buf[idx + 1..].to_string();
                        self.in_single_bracket = false;
                    }
                    None => {
                        // Whole tail is inside the marker. Drop it all.
                        self.buf.clear();
                        break;
                    }
                }
                continue;
            }
            // If we're inside an inline [[ marker, drop until ]].
            if self.in_bracket {
                let closer = "]]";
                match self.buf.find(closer) {
                    Some(idx) => {
                        let after = idx + closer.len();
                        self.buf = self.buf[after..].to_string();
                        self.in_bracket = false;
                    }
                    None => {
                        let keep = closer.len() - 1;
                        if self.buf.len() > keep {
                            let cut = self.buf.len() - keep;
                            let mut k = cut;
                            while k > 0 && !self.buf.is_char_boundary(k) {
                                k -= 1;
                            }
                            self.buf = self.buf[k..].to_string();
                        }
                        break;
                    }
                }
                continue;
            }

            // Find the next interesting position: any opener.
            //   `<citations>`  -> block until `</citations>`
            //   `[[`           -> drop until `]]`
            //   `[doc:`        -> drop until `]` (synth model uses this too)
            let next_cit = self.buf.find("<citations>");
            let next_dbl = self.buf.find("[[");
            let next_sgl = find_single_doc_bracket(&self.buf);
            let pos = [next_cit, next_dbl, next_sgl]
                .into_iter()
                .flatten()
                .min();

            if let Some(p) = pos {
                // Emit safe prefix UP TO (but not including) the opener.
                let (safe, tail) = split_safe(&self.buf[..p]);
                emit.push_str(safe);
                let opener_and_rest = self.buf[p..].to_string();
                self.buf = format!("{tail}{opener_and_rest}");

                if self.buf.starts_with("<citations>") {
                    self.buf = self.buf["<citations>".len()..].to_string();
                    self.in_citations = true;
                    continue;
                }
                if self.buf.starts_with("[[") {
                    self.buf = self.buf[2..].to_string();
                    self.in_bracket = true;
                    continue;
                }
                if self.buf.starts_with("[doc:") {
                    // Drop the `[`; rely on the in_single_bracket state to
                    // skip until the matching `]`.
                    self.buf = self.buf[1..].to_string();
                    self.in_single_bracket = true;
                    continue;
                }
                break;
            }

            // No opener found in the current buffer; emit the safe prefix
            // and hold back the rest as the tail (because the buffer's last
            // few bytes might be the start of a delimiter).
            let (safe, tail) = split_safe(&self.buf);
            emit.push_str(safe);
            self.buf = tail.to_string();
            break;
        }

        emit
    }

    /// Flush whatever is left in the buffer at end-of-stream. Anything still
    /// "in" a delimiter is discarded; partial-looking tails are emitted.
    fn flush(&mut self) -> String {
        if self.in_citations || self.in_bracket || self.in_single_bracket {
            self.buf.clear();
            return String::new();
        }
        std::mem::take(&mut self.buf)
    }
}

/// Find the position of the next `[doc:` not preceded by another `[`
/// (which would be the double-bracket form we handle separately).
fn find_single_doc_bracket(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let needle = b"[doc:";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Skip if preceded by another `[` (double-bracket case).
            if i == 0 || bytes[i - 1] != b'[' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Split `s` into (safe_prefix, suspicious_tail) where suspicious_tail is
/// the largest suffix that could be the beginning of any opener.
/// We hold the tail back until more bytes arrive to disambiguate.
fn split_safe(s: &str) -> (&str, &str) {
    const OPENERS: &[&str] = &["<citations>", "[[", "[doc:"];
    let max_open = OPENERS.iter().map(|o| o.len()).max().unwrap_or(0);
    let mut cut = s.len();
    let scan_start = s.len().saturating_sub(max_open);
    // Try each starting position in the suffix; if the substring s[i..] is
    // a prefix of any opener, hold from i onward.
    for i in scan_start..s.len() {
        // Use char boundary if multi-byte.
        if !s.is_char_boundary(i) {
            continue;
        }
        let tail = &s[i..];
        for o in OPENERS {
            if !tail.is_empty() && o.starts_with(tail) {
                cut = i;
                return (&s[..cut], &s[cut..]);
            }
        }
    }
    (&s[..cut], "")
}

/// Pull all unique node-id strings out of citation markers in the
/// accumulated answer text. Handles `[[doc:.../leaf:N]]`, `[doc:.../leaf:N]`,
/// and the closing `<citations>id1,id2,...</citations>` form.
fn extract_inline_node_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Double-bracket form [[doc:...]].
        if i + 1 < bytes.len() && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end_rel) = text[i + 2..].find("]]") {
                let inner = text[i + 2..i + 2 + end_rel].trim();
                if inner.starts_with("doc:") && seen.insert(inner.to_string()) {
                    out.push(inner.to_string());
                }
                i += 2 + end_rel + 2;
                continue;
            }
            break;
        }
        // Single-bracket form [doc:...].
        if bytes[i] == b'[' && text[i + 1..].starts_with("doc:") {
            if let Some(end_rel) = text[i + 1..].find(']') {
                let inner = text[i + 1..i + 1 + end_rel].trim();
                if seen.insert(inner.to_string()) {
                    out.push(inner.to_string());
                }
                i += 1 + end_rel + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    if let Some(start) = text.rfind("<citations>") {
        if let Some(end_rel) = text[start..].find("</citations>") {
            let inner = &text[start + "<citations>".len()..start + end_rel];
            for raw in inner.split(',') {
                let id = raw.trim().to_string();
                if id.starts_with("doc:") && seen.insert(id.clone()) {
                    out.push(id);
                }
            }
        }
    }
    out
}

/// Walk a `doc:.../leaf:N` node id back to its document's title via
/// pagebridge.get_node. Returns a JSON-shaped citation payload, or None
/// if pagebridge cannot resolve the id.
async fn lookup_citation(
    bridge: &Arc<pagebridge::Pagebridge>,
    node_id_str: &str,
) -> Option<serde_json::Value> {
    let node_id: pagebridge::NodeId = match node_id_str.parse() {
        Ok(id) => id,
        Err(_) => return None,
    };
    let node = bridge.storage().get_node(&node_id).await.ok().flatten()?;
    // Excerpt: take the routing_summary (short) or first 240 chars of summary.
    let excerpt = if !node.routing_summary.trim().is_empty() {
        node.routing_summary.clone()
    } else {
        node.summary.chars().take(240).collect::<String>()
    };
    Some(json!({
        "node_id": node_id_str,
        "doc_id": node.doc_id.to_string(),
        "doc_title": node.title.clone(),
        "section_title": node.title,
        "page_range": node.page_start.zip(node.page_end),
        "excerpt": excerpt,
    }))
}

fn node_id_short(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_string()
}

fn sanitize_for_fts5(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        let keep = c.is_alphanumeric() || c == '_' || c == ',' || c == '.';
        if keep {
            out.push(c);
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{sanitize_for_fts5, StreamFilter};

    fn run(chunks: &[&str]) -> String {
        let mut f = StreamFilter::default();
        let mut out = String::new();
        for c in chunks {
            out.push_str(&f.push(c));
        }
        out.push_str(&f.flush());
        out
    }

    #[test]
    fn filter_strips_inline_brackets() {
        assert_eq!(
            run(&["Raft elects a leader [[doc:abc/leaf:1]]. Done."]),
            "Raft elects a leader . Done."
        );
    }

    #[test]
    fn filter_strips_split_brackets() {
        assert_eq!(
            run(&["Raft elects a leader [[doc:abc", "/leaf:1]]. Done."]),
            "Raft elects a leader . Done."
        );
    }

    #[test]
    fn filter_strips_citations_block() {
        assert_eq!(
            run(&["Answer text.\n<citations>doc:a,doc:b</citations>tail"]),
            "Answer text.\ntail"
        );
    }

    #[test]
    fn filter_strips_split_citations_block() {
        // The closing tag arrives in a separate chunk.
        let chunks = &["Answer text.\n<cita", "tions>doc:a</cita", "tions> trailing"];
        assert_eq!(run(chunks), "Answer text.\n trailing");
    }

    #[test]
    fn filter_strips_single_doc_brackets() {
        assert_eq!(
            run(&["See [doc:abc/leaf:1] for details."]),
            "See  for details."
        );
    }

    #[test]
    fn filter_strips_single_doc_brackets_split() {
        assert_eq!(
            run(&["See [doc:abc", "/leaf:1] for details."]),
            "See  for details."
        );
    }

    #[test]
    fn filter_does_not_strip_ordinary_brackets() {
        // [link] and [foo] are normal text and must pass through.
        assert_eq!(
            run(&["a [link] and [foo] survive."]),
            "a [link] and [foo] survive."
        );
    }

    #[test]
    fn filter_handles_lone_lt_at_buffer_edge() {
        // A `<` at the end of one chunk is held back until disambiguated.
        let chunks = &["look at this <", "thing>"];
        assert_eq!(run(chunks), "look at this <thing>");
    }

    #[test]
    fn strips_question_mark() {
        assert_eq!(
            sanitize_for_fts5("what is this document about?"),
            "what is this document about"
        );
    }

    #[test]
    fn strips_fts5_operators() {
        assert_eq!(sanitize_for_fts5("foo+bar*baz"), "foo bar baz");
        assert_eq!(sanitize_for_fts5("a:b^c-d"), "a b c d");
        assert_eq!(sanitize_for_fts5("\"quoted\""), "quoted");
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(sanitize_for_fts5("  a   ?  b  "), "a b");
    }

    #[test]
    fn keeps_numbers_and_basic_punct() {
        assert_eq!(
            sanitize_for_fts5("section 3.2, page 12."),
            "section 3.2, page 12."
        );
    }
}

fn chunk_stream_to_sse<S>(
    stream: S,
    bridge: Arc<pagebridge::Pagebridge>,
    history: Arc<History>,
    library: String,
    question: String,
) -> impl Stream<Item = Result<Event, Infallible>>
where
    S: Stream<Item = pagebridge::Result<AnswerChunk>> + Send + 'static,
{
    async_stream::stream! {
        let mut s = Box::pin(stream);
        let mut answer_text = String::new();
        let mut citations_acc: Vec<serde_json::Value> = Vec::new();
        let mut seen_node_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut trace_payload: Option<serde_json::Value> = None;
        let mut errored = false;
        let mut filter = StreamFilter::default();

        while let Some(item) = s.next().await {
            match item {
                Ok(AnswerChunk::Token { text }) => {
                    answer_text.push_str(&text);
                    // Forward a CLEANED version of the token to the UI:
                    // strip inline [[doc:.../leaf:N]] markers and the trailing
                    // <citations>...</citations> block. Citations land in the
                    // citations panel via the synthesised events below.
                    let cleaned = filter.push(&text);
                    if !cleaned.is_empty() {
                        if let Ok(ev) = sse::event_json("token", &json!({ "text": cleaned })) {
                            yield Ok::<_, Infallible>(ev);
                        }
                    }
                }
                Ok(AnswerChunk::Citation { citation }) => {
                    let id = citation.node_id.to_string();
                    if seen_node_ids.insert(id.clone()) {
                        let payload = json!({
                            "node_id": id,
                            "doc_id": citation.doc_id.to_string(),
                            "doc_title": citation.doc_title,
                            "section_title": citation.section_title,
                            "page_range": citation.page_range,
                            "excerpt": citation.excerpt,
                        });
                        citations_acc.push(payload.clone());
                        if let Ok(ev) = sse::event_json("citation", &payload) {
                            yield Ok::<_, Infallible>(ev);
                        }
                    }
                }
                Ok(AnswerChunk::Done { trace, citations }) => {
                    // Flush any held-back tail from the filter.
                    let tail = filter.flush();
                    if !tail.is_empty() {
                        if let Ok(ev) = sse::event_json("token", &json!({ "text": tail })) {
                            yield Ok::<_, Infallible>(ev);
                        }
                    }
                    // If the model emitted inline [[doc:.../leaf:N]] markers
                    // but pagebridge never structured them as Citation chunks,
                    // recover them now by parsing the accumulated answer.
                    for node_id in extract_inline_node_ids(&answer_text) {
                        if !seen_node_ids.insert(node_id.clone()) {
                            continue;
                        }
                        let cit = lookup_citation(&bridge, &node_id).await;
                        let payload = cit.unwrap_or_else(|| json!({
                            "node_id": node_id,
                            "doc_id": "",
                            "doc_title": "",
                            "section_title": node_id_short(&node_id),
                            "page_range": serde_json::Value::Null,
                            "excerpt": "",
                        }));
                        citations_acc.push(payload.clone());
                        if let Ok(ev) = sse::event_json("citation", &payload) {
                            yield Ok::<_, Infallible>(ev);
                        }
                    }

                    let payload = json!({
                        "query_id": trace.query_id,
                        "duration_ms": trace.duration_ms,
                        "total_input_tokens": trace.total_input_tokens,
                        "total_output_tokens": trace.total_output_tokens,
                        "citation_count": citations.len().max(citations_acc.len()),
                    });
                    trace_payload = Some(payload.clone());
                    if let Ok(ev) = sse::event_json("done", &payload) {
                        yield Ok::<_, Infallible>(ev);
                    }
                    break;
                }
                Err(e) => {
                    errored = true;
                    let payload = json!({ "error": e.to_string() });
                    if let Ok(ev) = sse::event_json("error", &payload) {
                        yield Ok::<_, Infallible>(ev);
                    }
                    break;
                }
            }
        }

        if !errored && !answer_text.trim().is_empty() {
            let record = AskRecord {
                id: ulid::Ulid::new().to_string(),
                library,
                question,
                answer: answer_text,
                citations_json: serde_json::Value::Array(citations_acc),
                trace_json: trace_payload.unwrap_or(serde_json::json!({})),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
                    .unwrap_or(0),
            };
            if let Err(e) = history.record(&record).await {
                tracing::warn!("history record failed: {e}");
            }
        }
    }
}
