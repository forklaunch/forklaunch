#[path = "persistence/compliance_schema.rs"]
mod compliance_schema;
use forklaunch_core::{build, persistence::compliance};

use std::{error::Error, fs, path::Path};
#[allow(dead_code)]
#[path = "domain/schemas/event_contract.rs"]
mod event_contract;
fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=persistence/compliance_schema.rs");
    println!("cargo:rerun-if-changed=.forklaunch/compliance.json");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".forklaunch/compliance.json");
    let metadata = compliance::compliance_manifest(&compliance_schema::declarations()).to_string();
    if fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) {
        fs::create_dir_all(path.parent().expect("crate-local artifact directory"))?;
        fs::write(path, metadata)?;
    }
    println!("cargo:rerun-if-changed=domain/schemas/event_contract.rs");
    let input = serde_json::to_value(schemars::schema_for!(event_contract::WorkInput))?;
    let event = serde_json::to_value(schemars::schema_for!(event_contract::WorkEvent))?;
    build::worker_events(env!("CARGO_PKG_NAME"), &input, &event,
        include_str!("domain/schemas/event_contract.rs"), "events")?;
    Ok(())
}
