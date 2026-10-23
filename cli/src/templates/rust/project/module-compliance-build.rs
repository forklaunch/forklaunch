#[path = "persistence/compliance_schema.rs"]
mod compliance_schema;
use forklaunch_core::persistence::compliance;

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
    Ok(())
}
