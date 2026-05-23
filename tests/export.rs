//! Integration test for /api/history/:id/export.

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

#[tokio::test]
async fn export_returns_markdown_with_footnotes() {
    let (addr, token, _dir, history, _h) = spawn().await;
    history
        .record(&AskRecord {
            id: "abc".into(),
            library: "reading".into(),
            question: "what is consensus?".into(),
            answer: "Consensus is agreement.".into(),
            citations_json: serde_json::json!([{
                "doc_title": "Raft paper",
                "section_title": "Section 5",
                "page_range": [12, 14],
                "excerpt": "Leader is elected each term."
            }]),
            trace_json: serde_json::json!({}),
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    let resp = reqwest::get(format!("http://{addr}/api/history/abc/export?t={token}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let ctype = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    assert!(ctype.starts_with("text/markdown"));
    let body = resp.text().await.unwrap();
    assert!(body.contains("# Q: what is consensus?"));
    assert!(body.contains("[^1]"));
    assert!(body.contains("Raft paper"));
}

#[tokio::test]
async fn export_unknown_id_returns_404() {
    let (addr, token, _dir, _history, _h) = spawn().await;
    let resp = reqwest::get(format!("http://{addr}/api/history/nope/export?t={token}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn export_bad_format_returns_400() {
    let (addr, token, _dir, history, _h) = spawn().await;
    history
        .record(&AskRecord {
            id: "x".into(),
            library: "reading".into(),
            question: "q".into(),
            answer: "a".into(),
            citations_json: serde_json::json!([]),
            trace_json: serde_json::json!({}),
            created_at: 0,
        })
        .await
        .unwrap();
    let resp = reqwest::get(format!(
        "http://{addr}/api/history/x/export?format=html&t={token}"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}
