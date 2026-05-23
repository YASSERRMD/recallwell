//! One-time URL token for protecting the server against drive-by access.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;

use crate::server::AppState;

const TOKEN_BYTES: usize = 24;
const TOKEN_QUERY_PARAM: &str = "t";
const TOKEN_HEADER: &str = "x-recallwell-token";

/// Generate a fresh one-time token, URL-safe base64 encoded.
pub fn new_token() -> String {
    let mut buf = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Middleware that requires `?t=<token>` or `X-Recallwell-Token: <token>`.
///
/// Paths under `/assets/` bypass the check.
pub async fn require_token(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if path.starts_with("/assets/") {
        return next.run(request).await;
    }

    let supplied_from_query = request.uri().query().and_then(|q| extract_query_token(q));
    let supplied_from_header = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let supplied = supplied_from_query.or(supplied_from_header);

    if let Some(token) = supplied {
        if constant_time_eq(token.as_bytes(), state.token.as_bytes()) {
            return next.run(request).await;
        }
    }

    (StatusCode::UNAUTHORIZED, Html(unauthorized_html())).into_response()
}

fn extract_query_token(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let key = it.next()?;
        let value = it.next()?;
        if key == TOKEN_QUERY_PARAM {
            return urlencoding_decode(value);
        }
    }
    None
}

fn urlencoding_decode(s: &str) -> Option<String> {
    // For token characters we only expect URL-safe base64 (no %-escapes).
    // Accept either way: pass through if no percent, else best-effort decode.
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16)?;
            let lo = (bytes[i + 2] as char).to_digit(16)?;
            out.push(u8::try_from(hi * 16 + lo).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn unauthorized_html() -> String {
    r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>recallwell: unauthorized</title>
<style>body{font-family:system-ui,sans-serif;max-width:640px;margin:4rem auto;padding:0 1rem;color:#333}h1{margin-bottom:.25rem}code{background:#f4f4f5;padding:.1rem .3rem;border-radius:.25rem}</style></head>
<body>
<h1>recallwell</h1>
<p>This URL is missing the one-time token.</p>
<p>Look in the terminal where you launched <code>recallwell</code>; it printed the full URL including <code>?t=...</code>. Use that URL.</p>
<p>If you closed the terminal, restart the server to get a fresh URL.</p>
</body></html>"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_url_safe_and_long() {
        let t = new_token();
        // URL_SAFE_NO_PAD of 24 bytes => 32 chars.
        assert_eq!(t.len(), 32);
        assert!(t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn two_tokens_differ() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
    }

    #[test]
    fn extract_query_token_picks_t() {
        assert_eq!(
            extract_query_token("foo=1&t=abc&bar=2").as_deref(),
            Some("abc")
        );
        assert_eq!(extract_query_token("nope=1").as_deref(), None);
    }
}
