//! Optional domain socket/event assets. Core HTTP SDK and compliance emission remains in build.rs.
use forklaunch_core::{build::{ServiceSdkPaths, provenance::GeneratedAssets}, contract::assembly::BindingDeclaration};

use std::error::Error;
#[allow(clippy::unnecessary_wraps)]
pub fn emit(
    _output: &ServiceSdkPaths,
) -> Result<(), Box<dyn Error>> {
    Ok(())
}

/// Explicitly owned outputs and extra inputs, captured before extension emission.
#[allow(clippy::unnecessary_wraps)]
pub fn declared_assets(
    _output: &ServiceSdkPaths,
) -> Result<Vec<GeneratedAssets>, Box<dyn Error>> {
    Ok(Vec::new())
}

/// Override with compiler-generated API group declarations to enable assembly verification.
#[allow(clippy::unnecessary_wraps)]
pub fn binding_declarations() -> Result<Option<Vec<BindingDeclaration>>, Box<dyn Error>> {
    Ok(Some(contracts::binding_manifest()?))
}
