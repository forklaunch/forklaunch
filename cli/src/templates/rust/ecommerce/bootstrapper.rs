//! Process lifecycle: contract export, migrations, the inventory worker,
//! telemetry, registration and serving.
use crate::{
    config::{ServiceConfig, StoreConfig, WorkerConfig},
    domain::services::OrderInventoryHandler,
    error::Result,
    persistence::database,
    registrations, server, write_contract_artifacts,
};
use forklaunch_core::{config, telemetry::otlp::TelemetryRuntime};
use forklaunch_worker_database::http;
use std::{env, sync::Arc};
use tokio::net::TcpListener;

pub async fn run() -> Result<()> {
    if matches!(config::optional::<String>("FORKLAUNCH_MODE")?.as_deref(), Some("openapi" | "contract")) {
        write_contract_artifacts(
            config::required::<String>("FORKLAUNCH_CONTRACT_OUTPUT")?,
            config::required::<String>("FORKLAUNCH_OPENAPI_OUTPUT")?,
        )?;
        return Ok(());
    }
    // Migrations need only the owner database URL, not runtime settings such as RUST_LOG.
    if env::args().skip(1).eq(["--migrate"]) {
        return database::migrate().await;
    }
    let telemetry = TelemetryRuntime::start(crate::SERVICE_NAME, env!("CARGO_PKG_VERSION")).await?;
    let result = start().await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

async fn start() -> Result<()> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if !arguments.is_empty() && arguments != ["--migrate"] && arguments != ["--worker"] {
        return Err("Expected no arguments, --migrate or --worker".into());
    }
    if arguments == ["--migrate"] {
        return database::migrate().await;
    }
    let store = StoreConfig::from_env()?;
    let stores = registrations::stores(&store).await?;
    if arguments == ["--worker"] {
        let pool = stores.pool.clone();
        let result = inventory_worker(WorkerConfig::from_env(store)?, stores).await;
        pool.close().await;
        return result;
    }
    let state = registrations::register(ServiceConfig::from_env(store)?, stores).await?;
    let listener = TcpListener::bind(state.config.bind_addr()).await?;
    println!("{} listening on http://{}", crate::SERVICE_NAME, listener.local_addr()?);
    server::serve(&state, listener, server::shutdown()).await?;
    state.close().await;
    Ok(())
}

/// Applies order events to stock until shutdown, with optional readiness and controls.
async fn inventory_worker(config: WorkerConfig, stores: registrations::Stores) -> Result<()> {
    println!("{} inventory worker ready", crate::SERVICE_NAME);
    let report = http::run(
        &stores.queue,
        &OrderInventoryHandler::new(Arc::new(stores.store), "order-events"),
        &config.runner,
        None,
        config.http.as_ref(),
        server::shutdown(),
    )
    .await?;
    println!(
        "inventory worker stopped: completed={} retried={} dead_lettered={}",
        report.worker.completed, report.worker.retried, report.worker.dead_lettered
    );
    Ok(())
}
