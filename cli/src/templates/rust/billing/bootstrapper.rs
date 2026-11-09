//! Process lifecycle: contract export, migrations, telemetry, registration and serving.
use crate::{config::ServiceConfig, error::Result, persistence::database, registrations, server, write_contract_artifacts};
use forklaunch_core::{config, telemetry::otlp::TelemetryRuntime};
use std::env;
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
    if !arguments.is_empty() && arguments != ["--migrate"] {
        return Err("Expected no arguments or --migrate".into());
    }
    if arguments == ["--migrate"] {
        return database::migrate().await;
    }
    let state = registrations::register(ServiceConfig::from_env()?).await?;
    let listener = TcpListener::bind(state.config.bind_addr()).await?;
    println!("{} listening on http://{}", crate::SERVICE_NAME, listener.local_addr()?);
    server::serve(&state, listener, server::shutdown()).await?;
    state.close().await;
    Ok(())
}
