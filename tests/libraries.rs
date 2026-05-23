//! Integration tests for /api/libraries.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use recallwell::config::Config;
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
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, token, dir, handle)
}

#[tokio::test]
async fn empty_list_initially() {
    let (addr, token, _dir, _h) = spawn().await;
    let resp = reqwest::get(format!("http://{addr}/api/libraries?t={token}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_then_list_then_switch_then_delete() {
    let (addr, token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();

    // Create
    let resp = client
        .post(format!("http://{addr}/api/libraries?t={token}"))
        .json(&serde_json::json!({ "name": "reading" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "reading");

    // List
    let resp = reqwest::get(format!("http://{addr}/api/libraries?t={token}"))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "reading");

    // Switch
    let resp = client
        .post(format!(
            "http://{addr}/api/libraries/reading/switch?t={token}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    // Active
    let resp = reqwest::get(format!("http://{addr}/api/libraries/active?t={token}"))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["active"], "reading");

    // Delete
    let resp = client
        .delete(format!("http://{addr}/api/libraries/reading?t={token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);

    // List again -> empty
    let resp = reqwest::get(format!("http://{addr}/api/libraries?t={token}"))
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn invalid_name_rejected() {
    let (addr, token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/api/libraries?t={token}"))
        .json(&serde_json::json!({ "name": "BadName" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_create_rejected() {
    let (addr, token, _dir, _h) = spawn().await;
    let client = reqwest::Client::new();
    let _ = client
        .post(format!("http://{addr}/api/libraries?t={token}"))
        .json(&serde_json::json!({ "name": "dup" }))
        .send()
        .await
        .unwrap();
    let resp = client
        .post(format!("http://{addr}/api/libraries?t={token}"))
        .json(&serde_json::json!({ "name": "dup" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}
