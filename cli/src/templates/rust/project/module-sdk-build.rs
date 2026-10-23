#[path = "persistence/compliance_schema.rs"]
mod compliance_schema;
use forklaunch_core::{build::{self, provenance::ServiceBuild}, persistence::compliance};

use std::{env, error::Error, fs, path::PathBuf};
#[allow(dead_code)]
#[path = "api/contracts/mod.rs"]
mod contracts;
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=persistence/compliance_schema.rs");
    println!("cargo:rerun-if-changed=.forklaunch/compliance.json");
    println!("cargo:rerun-if-changed=api/contracts");
    for variable in ["FORKLAUNCH_TYPESCRIPT_SDK_DIR", "FORKLAUNCH_RUST_SDK_FILE", "BILLING_REQUEST_DATE_PROFILE", "BILLING_IDENTITY_PROFILE", "BILLING_BOUND_CHECKOUT_CONFIG", "BILLING_BOUND_BILLING_CONFIG", "BILLING_PUBLISHER_DATABASE_URL", "BILLING_HMAC_PROFILE", "BILLING_PAYMENT_LINK_UPDATE_PROFILE"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let path = root.join(".forklaunch/compliance.json");
    let metadata = compliance::compliance_manifest(&compliance_schema::declarations()).to_string();
    if fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) {
        fs::create_dir_all(path.parent().expect("crate-local artifact directory"))?;
        fs::write(path, metadata)?;
    }
    let registry = contracts::registry::configured_registry()?;
    let evidence = ServiceBuild::capture(&[])?;
    let output = build::service_sdks(&registry, "{{{name}}}", "ForkLaunchClient")?;
    evidence.record("{{{name}}}", &output)?;
    Ok(())
}
