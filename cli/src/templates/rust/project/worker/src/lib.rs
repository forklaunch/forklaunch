pub mod config;
pub mod error;
pub mod jobs;

use config::WorkerConfig;
use error::Result;
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, RouteContract, SchemaRef, ServiceContract,
};
use jobs::JobReport;
use std::{fs, path::Path, io};
use forklaunch_core::openapi::{self, OpenApiOptions};

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
    let mut registry = ContractRegistry::for_service(ServiceContract::new(
        WORKER_NAME,
        WORKER_VERSION,
    ));
    registry.register(
        RouteContract::get("Get Worker Health", "Reports worker health", "/health")
            .with_response(200, Some(worker_health_body())),
    );
    registry.register(
        RouteContract::post("Process Worker Jobs", "Processes one worker batch", "/jobs/process")
            .with_response(202, Some(job_report_body())),
    );
    registry
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

fn worker_health_body() -> BodyContract {
    BodyContract {
        kind: BodyKind::Json,
        schema: SchemaRef::named("WorkerHealthResponse").with_json_schema(
            r#"{"type":"object","required":["worker","status"],"properties":{"worker":{"type":"string"},"status":{"type":"string","const":"ok"}}}"#,
        ),
        content_type: Some("application/json".to_string()),
    }
}

fn job_report_body() -> BodyContract {
    BodyContract {
        kind: BodyKind::Json,
        schema: SchemaRef::named("JobReport").with_json_schema(
            r#"{"type":"object","required":["worker","processed","message"],"properties":{"worker":{"type":"string"},"processed":{"type":"integer","minimum":0},"message":{"type":"string"}}}"#,
        ),
        content_type: Some("application/json".to_string()),
    }
}
