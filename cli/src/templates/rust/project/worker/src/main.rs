use std::{fs, path::Path, error::Error, io};

use {{crate_module}}::{
    WORKER_NAME, build_context, config::WorkerConfig, run_once, write_contract_artifacts,
};
use forklaunch_core::{config, telemetry::otlp::TelemetryRuntime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if export_contract_artifacts()? {
        return Ok(());
    }

    let telemetry = TelemetryRuntime::start(
        WORKER_NAME, env!("CARGO_PKG_VERSION"),
    ).await?;
    let result = run();
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let context = build_context(WorkerConfig::from_env()?)?;
    let report = run_once(&context)?;
    println!(
        "{} processed {} job(s): {}",
        WORKER_NAME, report.processed, report.message
    );
    Ok(())
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
