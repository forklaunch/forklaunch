pub mod config;
pub mod error;
pub mod jobs;
pub mod handler;
pub mod event_contract;
pub mod events;

use config::WorkerConfig;
use error::Result;
use forklaunch_core::{contract::{ContractRegistry, ServiceContract}, openapi::{self, OpenApiOptions}};
use jobs::JobReport;
use std::{fs, path::Path, io};

pub const WORKER_NAME: &str = "{{{name}}}";
pub const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct WorkerContext {
    config: WorkerConfig,
}

impl WorkerContext {
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }
}

pub fn build_context(config: WorkerConfig) -> Result<WorkerContext> {
    config.validate()?;
    Ok(WorkerContext { config })
}

pub fn run_once(context: &WorkerContext) -> Result<JobReport> {
    jobs::process_next(context)
}

#[must_use]
pub fn contract_registry() -> ContractRegistry {
    // Durable queues run handlers directly. Internal management HTTP is opt-in
    // and is deliberately separate from application contracts and SDKs.
    ContractRegistry::for_service(ServiceContract::new(WORKER_NAME, WORKER_VERSION))
}

pub fn write_contract_artifacts(
    contract_output: impl AsRef<Path>,
    openapi_output: impl AsRef<Path>,
) -> io::Result<()> {
    let registry = contract_registry();

    if let Some(parent) = contract_output.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(contract_output, registry.clone().into_artifact().to_json())?;

    openapi::write_openapi_json(
        &registry,
        &OpenApiOptions::from_registry(&registry),
        openapi_output,
    )
}
