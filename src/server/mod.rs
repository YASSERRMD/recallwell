//! axum server bootstrap and shared state for recallwell.

pub mod auth;
pub mod error;
pub mod routes;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::middleware;
use tokio::net::TcpListener;
use tracing::info;

use crate::config::Config;

/// Shared application state.
pub struct AppState {
    pub config: Arc<Config>,
    pub token: String,
    pub started_at: std::time::Instant,
}

/// Result of binding the server: addr to listen on plus the token.
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub token: String,
    pub url: String,
}

const MAX_PORT_ATTEMPTS: u16 = 5;

/// Start the recallwell HTTP server.
///
/// Returns when the server shuts down (e.g. on Ctrl-C).
pub async fn run(config: Arc<Config>) -> Result<()> {
    let token = auth::new_token();
    let listener = bind_with_fallback(&config.server.host, config.server.port)
        .await
        .context("binding HTTP listener")?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/?t={token}");

    info!("recallwell listening on {addr}");

    let state = Arc::new(AppState {
        config: config.clone(),
        token: token.clone(),
        started_at: std::time::Instant::now(),
    });

    let app = routes::router(state.clone()).layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_token,
    ));

    print_banner(env!("CARGO_PKG_VERSION"), &url, config.server.auto_open);

    if config.server.auto_open {
        if let Err(err) = opener::open(&url) {
            eprintln!("(could not open browser automatically: {err})");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum serve")?;

    Ok(())
}

async fn bind_with_fallback(host: &str, start_port: u16) -> Result<TcpListener> {
    let mut last_err = None;
    for offset in 0..MAX_PORT_ATTEMPTS {
        let port = start_port.saturating_add(offset);
        let addr = format!("{host}:{port}");
        match TcpListener::bind(&addr).await {
            Ok(listener) => return Ok(listener),
            Err(e) => {
                last_err = Some(anyhow::anyhow!("bind {addr}: {e}"));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("could not bind any port near {start_port}")))
}

fn print_banner(version: &str, url: &str, auto_open: bool) {
    println!("recallwell v{version}");
    println!("Server running at {url}");
    println!();
    if auto_open {
        println!("Browser opening automatically. Bookmark this URL for this session.");
    } else {
        println!("Open this URL in your browser. Bookmark it for this session.");
    }
    println!("Press Ctrl+C to stop.");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    info!("shutdown signal received");
}
