//! `forklaunch compliance scan` — what this code stores, read from the source
//! and nothing else: the regulated fields of each entity (classification,
//! encryption, retention) and every cache and object-store write.
//!
//! It is the same scan a release carries. Run on its own it reads no account,
//! contacts no service and executes none of the application's code, so the
//! platform can run it against the commit an environment is running when that
//! release was created before releases carried a scan.

use std::path::Path;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;

use crate::{
    CliCommand,
    core::{
        command::command, static_analysis::storage_analyzer::StorageUsage,
        validate::require_manifest,
    },
};

/// A release's regulated fields and storage use, scanned from its source.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FieldInventory {
    pub(crate) generated_at: String,
    pub(crate) entities: Vec<super::audit::EntityReport>,
    /// Cache and object-store writes found in the code: what each service keeps in a
    /// cache (and for how long) or a bucket, and whether it is encrypted per tenant.
    pub(crate) storage: Vec<StorageUsage>,
}

pub(crate) fn scan_field_inventory(modules_path: &Path) -> Result<FieldInventory> {
    Ok(FieldInventory {
        generated_at: chrono::Utc::now().to_rfc3339(),
        entities: super::audit::scan_field_inventory(modules_path)?,
        storage: crate::core::static_analysis::storage_analyzer::scan_storage_usage(modules_path),
    })
}

#[derive(Debug)]
pub(crate) struct ScanCommand;

impl CliCommand for ScanCommand {
    fn command(&self) -> Command {
        command(
            "scan",
            "Scan this code for the data it stores: regulated fields, and cache and object-store writes. Reads source only; needs no login",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Path to application root (optional)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Write the scan as JSON to this file instead of printing it"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Print the scan as JSON, in the shape a release carries"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (app_root, manifest) = require_manifest(matches)?;
        let inventory = scan_field_inventory(&app_root.join(&manifest.modules_path))?;
        if let Some(output) = matches.get_one::<String>("output") {
            std::fs::write(output, serde_json::to_string(&inventory)?)?;
            return Ok(());
        }
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string(&inventory)?);
            return Ok(());
        }
        let fields: usize = inventory.entities.iter().map(|e| e.fields.len()).sum();
        println!(
            "{} entities with {} classified fields; {} cache or object-store writes.",
            inventory.entities.len(),
            fields,
            inventory.storage.len()
        );
        println!("Run with --json for the full scan.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        ScanCommand.command().version("0.0.0-test").debug_assert();
    }

    #[test]
    fn scan_has_the_shape_a_release_carries() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = scan_field_inventory(dir.path()).unwrap();
        let json = serde_json::to_value(&inventory).unwrap();
        assert!(json["generatedAt"].is_string());
        assert!(json["entities"].is_array());
        assert!(json["storage"].is_array());
    }
}
