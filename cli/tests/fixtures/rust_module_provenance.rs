//! Read-only verification of the exact scope declared by a generated build.
use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    while let Some(directory) = arguments.next() {
        let directory = PathBuf::from(directory);
        let receipt = PathBuf::from(arguments.next().ok_or("Missing provenance receipt")?);
        let manifest: forklaunch_core::build::provenance::Manifest =
            serde_json::from_slice(&fs::read(receipt)?)?;
        manifest.verify_service_scope(&directory, &manifest.context)?;
        println!("PASS declared build provenance: {}", manifest.service);
    }
    Ok(())
}
