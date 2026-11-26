use super::extensions as routes;

use forklaunch_billing_stripe::http::{BillingIdentityProfile, BillingRequestDateProfile, BoundBillingConfig, profiles};
use forklaunch_core::{
    ServiceInfo,
    contract::{ContractRegistry, ServiceContract},
    domain::{BlueprintError, BlueprintResult},
    openapi::{self, OpenApiOptions},
};
use std::{
    fs,
    io::{self, Error, ErrorKind},
    path::Path,
};

pub const SERVICE_NAME: &str = "{{{name}}}";
pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Whether this module bills through Stripe.
pub const STRIPE: bool = {{#stripe}}true{{/stripe}}{{^stripe}}false{{/stripe}};

pub fn service_info() -> ServiceInfo {
    ServiceInfo::new(SERVICE_NAME, SERVICE_VERSION)
}

pub fn contract_registry() -> ContractRegistry {
    contract_registry_with_profiles(BillingRequestDateProfile::Legacy, BillingIdentityProfile::Legacy)
        .expect("package-owned billing request schemas")
}

/// Explicit aggregate registry selection requires the same finite configuration as startup.
pub fn contract_registry_with_bound_billing(
    profile: BillingRequestDateProfile,
    configuration: &BoundBillingConfig,
) -> BlueprintResult<ContractRegistry> {
    let mut registry = contract_registry_with_profiles(profile, BillingIdentityProfile::Legacy)?;
    profiles::register_bound_billing(&mut registry, STRIPE, configuration)?;
    Ok(registry)
}

pub fn contract_registry_with_profiles(
    profile: BillingRequestDateProfile,
    identity: BillingIdentityProfile,
) -> BlueprintResult<ContractRegistry> {
    let mut registry = profiles::module_registry(ServiceContract::new(SERVICE_NAME, SERVICE_VERSION), STRIPE, profile, identity)?;
    for route in routes::contracts() {
        registry.register(route);
    }
    Ok(registry)
}

pub fn write_contract_artifacts(contract_output: impl AsRef<Path>, openapi_output: impl AsRef<Path>) -> io::Result<()> {
    let registry = configured_registry()?;
    if let Some(parent) = contract_output.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(contract_output, registry.clone().into_artifact().to_json())?;
    openapi::write_openapi_json(&registry, &OpenApiOptions::from_registry(&registry), openapi_output)
}

/// The registry the process environment selects, as service startup selects it.
pub fn configured_registry() -> io::Result<ContractRegistry> {
    let invalid = |error: BlueprintError| Error::new(ErrorKind::InvalidInput, error.to_string());
    let profile = profiles::request_date_profile_from_env().map_err(invalid)?;
    let identity = profiles::identity_profile_from_env(STRIPE).map_err(invalid)?;
    let registry = match profiles::bound_billing_config_from_env(STRIPE).map_err(invalid)? {
        Some(configuration) => contract_registry_with_bound_billing(profile, &configuration),
        None => contract_registry_with_profiles(profile, identity),
    }
    .map_err(invalid)?;
    Ok(registry)
}
