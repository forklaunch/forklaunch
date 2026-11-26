//! Process configuration, resource registration and startup under the framework lifecycle.
use crate::{config::ServiceConfig, registrations, server};
use forklaunch_axum::native::{self, ApplicationResult, Shutdown};
use std::env;
use tokio::net::TcpListener;

pub async fn run() -> ApplicationResult {
    native::run_application("{{{name}}}", env!("CARGO_PKG_VERSION"), &contracts::registry(), start()).await
}

async fn start() -> ApplicationResult {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if !arguments.is_empty() { return Err("Expected no arguments".into()); }
    let mut config = ServiceConfig::from_env()?;
    config.validate()?;
    let listener = TcpListener::bind(config.bind_addr()).await?;
    config.set_bound_port(listener.local_addr()?.port());
    let state = registrations::register(config)?;
    let stopping = Shutdown::new();
    println!("{{{name}}} listening on http://{}", listener.local_addr()?);
    server::serve(state, listener, stopping.signal()).await
}
