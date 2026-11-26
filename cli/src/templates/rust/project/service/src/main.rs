use std::{fs, path::Path, error::Error, io};

use {{crate_module}}::{build_state, config::ServiceConfig, router, write_contract_artifacts};
use forklaunch_core::{config, telemetry::otlp::TelemetryRuntime};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if export_contract_artifacts()? {
        return Ok(());
    }

    let telemetry = TelemetryRuntime::start(
        {{crate_module}}::SERVICE_NAME, env!("CARGO_PKG_VERSION"),
    ).await?;
    let result = run().await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let config = ServiceConfig::from_env()?;
    let listener = TcpListener::bind(config.bind_addr()).await?;
    let state = build_state(config)?;
    println!("{} listening on http://{}", {{crate_module}}::SERVICE_NAME, listener.local_addr()?);
    router(state).serve_with_default_mcp(listener, shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

/// The CLI's contract export: `FORKLAUNCH_MODE` selects it and both output
/// locations are then required.
fn export_contract_artifacts() -> io::Result<bool> {
    let mode: Option<String> = config::optional("FORKLAUNCH_MODE")?;
    if !matches!(mode.as_deref(), Some("openapi" | "contract")) {
        return Ok(false);
    }

    let contract_output: String = config::required("FORKLAUNCH_CONTRACT_OUTPUT")?;
    let openapi_output: String = config::required("FORKLAUNCH_OPENAPI_OUTPUT")?;

    if let Some(parent) = Path::new(&openapi_output).parent() {
        fs::create_dir_all(parent)?;
    }
    write_contract_artifacts(contract_output, openapi_output)?;
    Ok(true)
}
