mod build_assets;
#[path = "persistence/compliance_schema.rs"]
mod compliance_schema;
use forklaunch_core::{build::{self, provenance::ServiceBuild}, persistence::compliance};

use std::{env, error::Error, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=persistence/compliance_schema.rs");
    println!("cargo:rerun-if-changed=.forklaunch/compliance.json");
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(".forklaunch/compliance.json");
    let metadata = compliance::compliance_manifest(&compliance_schema::declarations()).to_string();
    if fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) {
        fs::create_dir_all(path.parent().expect("crate-local artifact directory"))?;
        fs::write(path, metadata)?;
    }
    println!("cargo:rerun-if-changed=domain/schemas");
    println!("cargo:rerun-if-changed=api/contracts");
    println!("cargo:rerun-if-changed=sdk.rs");
    println!("cargo:rerun-if-changed=build_assets.rs");
    let evidence = ServiceBuild::capture(&[])?;
    let output = build::service_sdks(&contracts::registry(), "{{{name}}}", "ForkLaunchClient")?;
    let evidence = evidence.include_assets(&build_assets::declared_assets(&output)?)?;
    let evidence = if let Some(bindings) = build_assets::binding_declarations()? { evidence.with_bindings(bindings) } else { evidence };
    build_assets::emit(&output)?;
    evidence.record("{{{name}}}", &output)?;
    Ok(())
}
