//! Shared framework HTTP/MCP security and listener lifecycle.
use crate::{registrations::AppState, application};
use forklaunch_axum::native::ApplicationResult;
use axum::Router;
use std::future::Future;
use tokio::net::TcpListener;
pub fn router(state: AppState) -> ApplicationResult<Router> { application::http(state)?.router() }
pub async fn serve<F>(state: AppState, listener: TcpListener, shutdown: F) -> ApplicationResult
where F: Future<Output = ()> + Send { application::http(state)?.serve(listener, shutdown).await }
