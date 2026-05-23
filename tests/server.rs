use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
use recallwell::server::{self, auth, routes, AppState};
use tokio::net::TcpListener;

/// Spawn the server on a random port and return (addr, token, shutdown).
async fn spawn_server() -> (SocketAddr, String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let token = auth::new_token();

    let state = Arc::new(AppState {
        config: Arc::new(Config::default()),
        token: token.clone(),
        started_at: std::time::Instant::now(),
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
    (addr, token, handle)
}

#[tokio::test]
async fn health_with_valid_token_returns_ok() {
    let (addr, token, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn health_without_token_is_unauthorized() {
    let (addr, _token, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_with_bad_token_is_unauthorized() {
    let (addr, _token, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/health?t=not-the-real-token");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_via_header_is_accepted() {
    let (addr, token, _h) = spawn_server().await;
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
    let (addr, token, _h) = spawn_server().await;
    let url = format!("http://{addr}/api/config?t={token}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    // Default config has no api key set; ensure key is None or redacted.
    assert!(body["groq"]["api_key"].is_null() || body["groq"]["api_key"] == "***redacted***");
}

#[allow(dead_code)]
fn _ensure_server_module_used() {
    // Force linkage of public modules for the integration crate.
    let _ = server::run;
}
