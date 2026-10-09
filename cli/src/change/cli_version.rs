//! Explicitly adopt the running CLI after the user reviews a project upgrade.
//! This command never installs tools, rewrites source, or runs project code.
use std::{fs, io::Write, path::Path};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use toml_edit::{DocumentMut, value};

use crate::{CliCommand, core::command::command};

#[derive(Debug)]
pub(super) struct CliVersionCommand;
fn stable_version(version: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        bail!("CLI pins must use an exact stable version, such as 1.28.0.");
    }
    Ok((parts[0].parse()?, parts[1].parse()?, parts[2].parse()?))
}
fn upgraded_manifest(input: &str, expected: &str, target: &str) -> Result<String> {
    if stable_version(target)? <= stable_version(expected)? {
        bail!("The selected CLI must be newer than the project's current pin.");
    }
    let mut doc: DocumentMut = input
        .parse()
        .context("Cannot parse the project manifest.")?;
    if doc.get("cli_version").and_then(|v| v.as_str()) != Some(expected) {
        bail!("The project's CLI pin changed since review. Review the upgrade again.");
    }
    let decor = doc["cli_version"].as_value().unwrap().decor().clone();
    doc["cli_version"] = value(target);
    *doc["cli_version"].as_value_mut().unwrap().decor_mut() = decor;
    Ok(doc.to_string())
}
fn adopt(root: &Path, expected: &str, target: &str, dry_run: bool) -> Result<()> {
    let root = root
        .canonicalize()
        .context("Application root is unavailable.")?;
    let directory = root.join(".forklaunch");
    if !fs::symlink_metadata(&directory)?.is_dir() {
        bail!("Project settings must be a regular directory.");
    }
    let path = directory.join("manifest.toml");
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        bail!("Project manifest must be a bounded regular file.");
    }
    let before = fs::read_to_string(&path)?;
    let after = upgraded_manifest(&before, expected, target)?;
    if !dry_run {
        let mut stage = tempfile::NamedTempFile::new_in(&directory)?;
        stage.as_file().set_permissions(meta.permissions())?;
        stage.write_all(after.as_bytes())?;
        stage.as_file().sync_all()?;
        if fs::read_to_string(&path)? != before {
            bail!("Project settings changed during upgrade. Review again.");
        }
        stage
            .persist(&path)
            .context("Cannot save the reviewed CLI pin.")?;
    }
    println!(
        "{} CLI pin: {} -> {}",
        if dry_run { "Reviewed" } else { "Updated" },
        expected,
        target
    );
    Ok(())
}
impl CliCommand for CliVersionCommand {
    fn command(&self) -> Command {
        command(
            "cli-version",
            "Explicitly upgrade a project's pin to this running CLI",
        )
        .arg(Arg::new("path").long("path").required(true))
        .arg(
            Arg::new("from")
                .long("from")
                .required(true)
                .help("Exact current pin reviewed by the user"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("confirm")
                .long("confirm")
                .action(ArgAction::SetTrue)
                .required_unless_present("dry-run"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        adopt(
            Path::new(matches.get_one::<String>("path").unwrap()),
            matches.get_one::<String>("from").unwrap(),
            env!("CARGO_PKG_VERSION"),
            matches.get_flag("dry-run"),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const MANIFEST: &str = "# customer configuration\ncli_version = \"1.27.0\" # reviewed pin\napp_name = \"appointments\"\n\n[[projects]]\nname = \"records\"\n";
    #[test]
    fn changes_only_pin_and_preserves_customer_settings() {
        let updated = upgraded_manifest(MANIFEST, "1.27.0", "1.28.0").unwrap();
        assert_eq!(updated, MANIFEST.replace("1.27.0", "1.28.0"));
        for (from, to) in [
            ("1.26.0", "1.28.0"),
            ("1.27.0", "1.27.0"),
            ("1.27.0", "1.26.0"),
            ("1.27.0", "latest"),
        ] {
            assert!(upgraded_manifest(MANIFEST, from, to).is_err());
        }
    }
    #[test]
    fn review_is_read_only_and_confirmed_upgrade_keeps_source() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".forklaunch")).unwrap();
        let path = root.path().join(".forklaunch/manifest.toml");
        fs::write(&path, MANIFEST).unwrap();
        fs::write(root.path().join("server.ts"), "customer code").unwrap();
        adopt(root.path(), "1.27.0", "1.28.0", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), MANIFEST);
        adopt(root.path(), "1.27.0", "1.28.0", false).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            MANIFEST.replace("1.27.0", "1.28.0")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("server.ts")).unwrap(),
            "customer code"
        );
        assert!(adopt(root.path(), "1.27.0", "1.28.0", false).is_err());
    }
    #[test]
    fn mutation_requires_explicit_confirmation() {
        let command = CliVersionCommand.command().version("1.28.0");
        assert!(
            command
                .clone()
                .try_get_matches_from(["cli-version", "--path", ".", "--from", "1.27.0"])
                .is_err()
        );
        assert!(
            command
                .try_get_matches_from([
                    "cli-version",
                    "--path",
                    ".",
                    "--from",
                    "1.27.0",
                    "--dry-run"
                ])
                .is_ok()
        );
    }
}

#[cfg(all(test, unix))]
mod linked_manifest_tests {
    use super::*;
    #[test]
    fn rejects_linked_customer_manifest_without_modifying_target() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "cli_version = \"1.27.0\"\n").unwrap();
        fs::create_dir(root.path().join(".forklaunch")).unwrap();
        std::os::unix::fs::symlink(
            outside.path(),
            root.path().join(".forklaunch/manifest.toml"),
        )
        .unwrap();
        assert!(adopt(root.path(), "1.27.0", "1.28.0", false).is_err());
        assert_eq!(
            fs::read_to_string(outside.path()).unwrap(),
            "cli_version = \"1.27.0\"\n"
        );
    }
}
