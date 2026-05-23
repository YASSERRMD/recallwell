//! Integration tests for /api/history.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
use recallwell::history::{AskRecord, History};
use recallwell::ingest::queue::IngestQueue;
use recallwell::library::LibraryRegistry;
use recallwell::server::{auth, routes, AppState};
use tempfile::TempDir;
use tokio::net::TcpListener;

async fn spawn() -> (
    SocketAddr,
    String,
    TempDir,
    Arc<History>,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let token = auth::new_token();

    let mut config = Config::default();
    config.data.dir = Some(dir.path().to_path_buf());
    config.groq.api_key = Some("gsk_test".into());
    let config = Arc::new(config);

    let libraries = Arc::new(LibraryRegistry::new(config.clone()).unwrap());
    let ingest = IngestQueue::start(libraries.clone(), config.clone(), 1);
    let history = Arc::new(
        History::open(&config.history_db_path().unwrap())
            .await
            .unwrap(),
    );

    let state = Arc::new(AppState {
        config,
        token: token.clone(),
        started_at: std::time::Instant::now(),
        libraries,
        ingest,
        history: history.clone(),
    });

    let app = routes::router(state.clone()).layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, token, dir, history, handle)
}

fn ask(id: &str, lib: &str, q: &str, ts: i64) -> AskRecord {
    AskRecord {
        id: id.into(),
        library: lib.into(),
        question: q.into(),
        answer: format!("answer to {q}"),
        citations_json: serde_json::json!([]),
        trace_json: serde_json::json!({}),
        created_at: ts,
    }
}

#[tokio::test]
async fn list_filter_get_search_delete() {
    let (addr, token, _dir, history, _h) = spawn().await;
    history
        .record(&ask("a", "reading", "consensus algorithms", 100))
        .await
        .unwrap();
    history
        .record(&ask("b", "work", "what did we ship?", 200))
        .await
        .unwrap();

    // List all
    let body: serde_json::Value = reqwest::get(format!("http://{addr}/api/history?t={token}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);

    // Filter by library
    let body: serde_json::Value = reqwest::get(format!(
        "http://{addr}/api/history?library=reading&t={token}"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["library"], "reading");

    // Get one
    let resp = reqwest::get(format!("http://{addr}/api/history/a?t={token}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Search
    let body: serde_json::Value = reqwest::get(format!(
        "http://{addr}/api/history/search?q=consensus&t={token}"
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["id"], "a");

    // Delete
    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("http://{addr}/api/history/a?t={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);

    let body: serde_json::Value = reqwest::get(format!("http://{addr}/api/history?t={token}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
}
