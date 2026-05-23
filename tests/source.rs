//! Integration test for /api/source/:library/:doc_id.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
use recallwell::history::History;
use recallwell::ingest::queue::IngestQueue;
use recallwell::library::LibraryRegistry;
use recallwell::server::{auth, routes, AppState};
use recallwell::source::{SourceEntry, SourceMap};
use tempfile::TempDir;
use tokio::net::TcpListener;

async fn spawn() -> (
    SocketAddr,
    String,
    TempDir,
    Arc<Config>,
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

    // Make the ingested-files dir up front so canonicalize works.
    std::fs::create_dir_all(config.ingested_files_dir().unwrap()).unwrap();

    let libraries = Arc::new(LibraryRegistry::new(config.clone()).unwrap());
    let ingest = IngestQueue::start(libraries.clone(), config.clone(), 1);
    let history = Arc::new(
        History::open(&config.history_db_path().unwrap())
            .await
            .unwrap(),
    );
    let state = Arc::new(AppState {
        config: config.clone(),
        token: token.clone(),
        started_at: std::time::Instant::now(),
        libraries,
        ingest,
        history,
    });

    let app = routes::router(state.clone()).layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, token, dir, config, handle)
}

#[tokio::test]
async fn unknown_doc_returns_404() {
    let (addr, token, _dir, _config, _h) = spawn().await;
    let url = format!("http://{addr}/api/source/default/missing?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn records_then_serves_file() {
    let (addr, token, _dir, config, _h) = spawn().await;

    // Set up a fake source file inside ingested-files dir.
    let library = "default";
    let job_dir = config
        .ingested_files_dir()
        .unwrap()
        .join(library)
        .join("job123");
    std::fs::create_dir_all(&job_dir).unwrap();
    let file_path = job_dir.join("sample.md");
    std::fs::write(&file_path, b"# Hello\n\nWorld.").unwrap();

    let map = SourceMap::open(config.clone(), library).unwrap();
    let entry = SourceEntry {
        file_path: file_path.clone(),
        original_filename: "sample.md".into(),
        ingested_at: 0,
    };
    map.record("doc-abc", entry).unwrap();

    let url = format!("http://{addr}/api/source/default/doc-abc?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(body.contains("World"));
}
