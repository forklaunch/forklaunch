//! Process lifecycle: contract export, telemetry, registration and serving.
use crate::{config::ServiceConfig, error::Result, registrations, server, write_contract_artifacts};
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
    let telemetry = TelemetryRuntime::start(crate::SERVICE_NAME, env!("CARGO_PKG_VERSION")).await?;
    let result = start().await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

async fn start() -> Result<()> {
    if env::args().len() != 1 {
        return Err("CAC expects no command arguments".into());
    }
    let state = registrations::register(ServiceConfig::from_env()?).await?;
    let listener = TcpListener::bind(state.config.bind_addr()).await?;
    println!("{} listening on http://{}", crate::SERVICE_NAME, listener.local_addr()?);
    server::serve(&state, listener, server::shutdown()).await?;
    Ok(())
}
