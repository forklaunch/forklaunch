
use super::extensions as routes;

use forklaunch_core::{
    ServiceInfo,
    contract::{ContractRegistry, ServiceContract},
    openapi::{self, OpenApiOptions},
};
use forklaunch_iam_interface::module_contracts;
use std::{fs, io, path::Path};

pub const SERVICE_NAME: &str = "{{{name}}}";
pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn service_info() -> ServiceInfo {
    ServiceInfo::new(SERVICE_NAME, SERVICE_VERSION)
}

pub fn contract_registry() -> ContractRegistry {
    let mut registry =
        ContractRegistry::for_service(ServiceContract::new(SERVICE_NAME, SERVICE_VERSION));
    for route in module_contracts::combined_registry().routes() {
        registry.register(route.clone());
    }
    for route in routes::contracts() {
        registry.register(route);
    }
    registry
}

pub fn write_contract_artifacts(
    contract_output: impl AsRef<Path>,
    openapi_output: impl AsRef<Path>,
) -> io::Result<()> {
    let registry = configured_registry()?;
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
pub fn configured_registry() -> io::Result<ContractRegistry> {
    let registry = contract_registry();
    Ok(registry)
}
