#[path = "src/persistence/compliance_schema.rs"]
mod compliance_schema;
use forklaunch_core::{build, persistence::compliance};

use std::{env, error::Error, fs, path::PathBuf};
#[allow(dead_code)]
#[path = "src/contracts/mod.rs"]
mod contracts;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=src/persistence/compliance_schema.rs");
    println!("cargo:rerun-if-changed=.forklaunch/compliance.json");
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(".forklaunch/compliance.json");
    let metadata = compliance::compliance_manifest(&compliance_schema::declarations()).to_string();
    if fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) {
        fs::create_dir_all(path.parent().expect("crate-local artifact directory"))?;
        fs::write(path, metadata)?;
    }
    println!("cargo:rerun-if-changed=src/contracts");
    println!("cargo:rerun-if-env-changed=FORKLAUNCH_TYPESCRIPT_SDK_DIR");
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let output = env::var_os("FORKLAUNCH_TYPESCRIPT_SDK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("../../sdks/typescript").join(env!("CARGO_PKG_NAME")));
    build::typescript(&contracts::registry(), env!("CARGO_PKG_NAME"), output)?;
    Ok(())
}
