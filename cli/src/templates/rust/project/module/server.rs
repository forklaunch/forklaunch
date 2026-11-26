//! Shared framework HTTP/MCP security and listener lifecycle.
use crate::{application, registrations::AppState};
use std::{future::Future, io};
use tokio::net::TcpListener;

pub async fn serve<F>(state: &AppState, listener: TcpListener, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send,
{
    application::http(state).serve_with_default_mcp(listener, shutdown).await
}

/// Resolves on Ctrl-C, or on SIGTERM where supported.
pub async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = signal.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
