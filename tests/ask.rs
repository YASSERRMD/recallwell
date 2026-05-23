//! Smoke tests for /api/ask error paths.
//!
//! We avoid making real Groq calls; full streaming behavior is exercised
//! manually in the UI and would require a live API key.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
use recallwell::ingest::queue::IngestQueue;
use recallwell::library::LibraryRegistry;
use recallwell::server::{auth, routes, AppState};
use tempfile::TempDir;
use tokio::net::TcpListener;

async fn spawn() -> (SocketAddr, String, TempDir, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let token = auth::new_token();

    let mut config = Config::default();
    config.data.dir = Some(dir.path().to_path_buf());
    config.groq.api_key = Some("gsk_test".into());
    let config = Arc::new(config);

    let libraries = Arc::new(LibraryRegistry::new(config.clone()).unwrap());
    let ingest = IngestQueue::start(libraries.clone(), 1);
    let state = Arc::new(AppState {
        config,
        token: token.clone(),
        started_at: std::time::Instant::now(),
        libraries,
        ingest,
    });

    let app = routes::router(state.clone()).layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, token, dir, handle)
}

#[tokio::test]
async fn empty_question_returns_400() {
    let (addr, token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/ask?t={token}"))
        .json(&serde_json::json!({ "question": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oversized_question_returns_400() {
    let (addr, token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();
    let huge = "x".repeat(5_000);
    let resp = client
        .post(format!("http://{addr}/api/ask?t={token}"))
        .json(&serde_json::json!({ "question": huge }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unauthorized_without_token() {
    let (addr, _token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/ask"))
        .json(&serde_json::json!({ "question": "hi" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}
