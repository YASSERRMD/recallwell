use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
use recallwell::library::LibraryRegistry;
use recallwell::server::{self, auth, routes, AppState};
use tempfile::TempDir;
use tokio::net::TcpListener;

/// Spawn the server on a random port and return (addr, token, _temp_dir, _join_handle).
/// The temp dir is kept alive by the caller (`_dir` binding) to back the data dir.
async fn spawn_server() -> (SocketAddr, String, TempDir, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let token = auth::new_token();

    let mut config = Config::default();
    config.data.dir = Some(dir.path().to_path_buf());
    // Pretend a Groq key is configured so library open works (we never call it).
    config.groq.api_key = Some("gsk_test".into());
    let config = Arc::new(config);

    let libraries = Arc::new(LibraryRegistry::new(config.clone()).unwrap());

    let state = Arc::new(AppState {
        config,
        token: token.clone(),
        started_at: std::time::Instant::now(),
        libraries,
    });

    let app = routes::router(state.clone()).layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // Give the server a moment to be ready.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, token, dir, handle)
}

#[tokio::test]
async fn health_with_valid_token_returns_ok() {
    let (addr, token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn health_without_token_is_unauthorized() {
    let (addr, _token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_with_bad_token_is_unauthorized() {
    let (addr, _token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health?t=not-the-real-token");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_via_header_is_accepted() {
    let (addr, token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("X-Recallwell-Token", token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn config_route_redacts_api_key() {
    let (addr, token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/config?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    // Default config has no api key set; ensure key is None or redacted.
    assert!(body["groq"]["api_key"].is_null() || body["groq"]["api_key"] == "***redacted***");
}

#[tokio::test]
async fn assets_served_without_auth() {
    let (addr, _token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/assets/recallwell.css");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert!(resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("")
        .starts_with("text/css"));
    let body = resp.text().await.unwrap();
    assert!(body.contains("rw-card"));
}

#[tokio::test]
async fn unknown_asset_returns_404() {
    let (addr, _token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/assets/evil.exe");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn index_renders_with_token_substituted() {
    let (addr, token, _dir, _h) = spawn_server().await;
    let url = format!("http://{addr}/?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(body.contains(&token));
    assert!(!body.contains("{TOKEN}"));
    assert!(body.contains("recallwell"));
}

#[allow(dead_code)]
fn _ensure_server_module_used() {
    // Force linkage of public modules for the integration crate.
    let _ = server::run;
}
