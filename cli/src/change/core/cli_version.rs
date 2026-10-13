use std::{env, fs, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::ArgMatches;
use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, Value};

use crate::core::base_path::find_nearest_manifest_from;

/// Released CLI tags use canonical numeric X.Y.Z versions. A development
/// version is not downloadable and must not become a new project pin.
pub(crate) fn parse_cli_version(input: &str) -> Result<String, String> {
    let parts: Vec<_> = input.split('.').collect();
    if input == "0.0.0"
        || parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u64>().is_err()
        })
    {
        return Err("Use a released CLI version in X.Y.Z form, for example 1.30.0; 0.0.0 is a development build".to_string());
    }
    Ok(input.to_string())
}

fn update_manifest_pin(manifest_path: &Path, version: &str, dryrun: bool) -> Result<String> {
    parse_cli_version(version).map_err(anyhow::Error::msg)?;
    let original = fs::read_to_string(manifest_path).context("Failed to read project manifest")?;
    let mut document: DocumentMut = original
        .parse()
        .context("Failed to parse project manifest")?;
    let old_value = document
        .get_mut("cli_version")
        .and_then(|item| item.as_value_mut())
        .context("Project manifest has no CLI version pin")?;
    let previous = old_value
        .as_str()
        .context("Project CLI version pin must be a string")?
        .to_string();
    let mut replacement = Value::from(version);
    *replacement.decor_mut() = old_value.decor().clone();
    *old_value = replacement;
    if !dryrun && previous != version {
        // Preserve every other field, comment and table, and replace atomically
        // so interruption cannot leave a partially written manifest.
        let parent = manifest_path
            .parent()
            .context("Manifest has no parent directory")?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary
            .as_file()
            .set_permissions(fs::metadata(manifest_path)?.permissions())?;
        temporary.write_all(document.to_string().as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(manifest_path)
            .context("Failed to save project CLI version pin")?;
    }
    Ok(previous)
}

pub(crate) fn change_cli_version(matches: &ArgMatches, version: &str) -> Result<()> {
    let start = match matches.get_one::<String>("base_path") {
        Some(path) => path.into(),
        None => env::current_dir()?,
    };
    // An invalid explicit path must not silently fall back to a different app
    // in the current directory.
    let root = find_nearest_manifest_from(&start)
        .context("No project manifest found at or above the requested path")?;
    let dryrun = matches.get_flag("dryrun");
    let previous = update_manifest_pin(&root.join(".forklaunch/manifest.toml"), version, dryrun)?;
    println!(
        "{} project CLI version from {} to {}. Other project settings are unchanged.",
        if dryrun { "Would change" } else { "Changed" },
        previous,
        version
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CliCommand,
        change::ChangeCommand,
        core::version_check::{VersionCheckOutcome, precheck_version},
    };

    #[test]
    fn accepts_only_canonical_released_version_numbers() {
        for version in ["1.30.0", "0.1.0", "20.2.123"] {
            assert_eq!(parse_cli_version(version).unwrap(), version);
        }
        for version in [
            "",
            "0.0.0",
            "v1.30.0",
            "1.30",
            "01.30.0",
            "1.30.0-beta",
            "1.30.0+build",
            "1.30.0/../../x",
            "1.30.0\n",
            "1.30.0;echo hi",
            "1.30.18446744073709551616",
        ] {
            assert!(parse_cli_version(version).is_err(), "accepted {version:?}");
        }
    }

    const MANIFEST: &str = "# Keep this project\ncli_version = '0.0.0' # pin\napp_name = \"pocket\"\nmodules_path = \"src/modules\"\nplatform_application_id = \"app-123\"\nplatform_organization_id = \"org-456\"\nfuture_setting = { enabled = true }\n\n[[projects]]\nname = \"iam\"\nfuture_field = [1, 2]\n";

    #[test]
    fn changes_only_the_pin_and_preserves_comments_and_unknown_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("manifest.toml");
        fs::write(&manifest, MANIFEST).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&manifest, fs::Permissions::from_mode(0o640)).unwrap();
        }
        assert_eq!(
            update_manifest_pin(&manifest, "1.30.0", false).unwrap(),
            "0.0.0"
        );
        assert_eq!(
            fs::read_to_string(&manifest).unwrap(),
            MANIFEST.replace("'0.0.0'", "\"1.30.0\"")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&manifest).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        update_manifest_pin(&manifest, "1.30.0", false).unwrap();
        assert_eq!(
            fs::read_to_string(&manifest).unwrap(),
            MANIFEST.replace("'0.0.0'", "\"1.30.0\"")
        );
    }

    #[test]
    fn dryrun_and_invalid_inputs_leave_the_manifest_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("manifest.toml");
        fs::write(&manifest, MANIFEST).unwrap();
        update_manifest_pin(&manifest, "1.30.0", true).unwrap();
        assert!(update_manifest_pin(&manifest, "0.0.0", false).is_err());
        assert_eq!(fs::read_to_string(&manifest).unwrap(), MANIFEST);
        for malformed in ["cli_version = 1\n", "app_name = 'pocket'\n", "[broken"] {
            fs::write(&manifest, malformed).unwrap();
            assert!(update_manifest_pin(&manifest, "1.30.0", false).is_err());
            assert_eq!(fs::read_to_string(&manifest).unwrap(), malformed);
        }
    }

    #[test]
    fn migration_is_noninteractive_and_bypasses_only_the_old_pin() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".forklaunch")).unwrap();
        let manifest = temp.path().join(".forklaunch/manifest.toml");
        fs::write(&manifest, MANIFEST).unwrap();
        let command = ChangeCommand::new();
        let matches = command
            .command()
            .version(env!("CARGO_PKG_VERSION"))
            .try_get_matches_from([
                "change",
                "application",
                "--path",
                temp.path().to_str().unwrap(),
                "--cli-version",
                "1.30.0",
            ])
            .unwrap();
        assert!(matches!(
            precheck_version(&matches, "change").unwrap(),
            VersionCheckOutcome::SkipWhitelisted
        ));
        command.handler(&matches).unwrap();
        assert!(
            fs::read_to_string(&manifest)
                .unwrap()
                .contains("cli_version = \"1.30.0\"")
        );
        // Use the executing version so this guard test never downloads an
        // older binary, including when tests run in a release-version build.
        fs::write(
            &manifest,
            MANIFEST.replace("0.0.0", env!("CARGO_PKG_VERSION")),
        )
        .unwrap();
        let ordinary = command
            .command()
            .version(env!("CARGO_PKG_VERSION"))
            .try_get_matches_from([
                "change",
                "application",
                "--path",
                temp.path().to_str().unwrap(),
                "--description",
                "Still pinned",
            ])
            .unwrap();
        // Ordinary changes remain subject to the normal version check.
        assert!(!matches!(
            precheck_version(&ordinary, "change").unwrap(),
            VersionCheckOutcome::SkipWhitelisted
        ));
    }

    #[test]
    fn rejects_combined_edits_and_invalid_target_without_changing_the_pin() {
        let command = ChangeCommand::new();
        assert!(
            command
                .command()
                .version(env!("CARGO_PKG_VERSION"))
                .try_get_matches_from([
                    "change",
                    "application",
                    "--cli-version",
                    "1.30.0",
                    "--runtime",
                    "bun"
                ])
                .is_err()
        );
        assert!(
            command
                .command()
                .version(env!("CARGO_PKG_VERSION"))
                .try_get_matches_from(["change", "application", "--cli-version", "0.0.0"])
                .is_err()
        );
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("does-not-exist");
        let matches = command
            .command()
            .version(env!("CARGO_PKG_VERSION"))
            .try_get_matches_from([
                "change",
                "application",
                "--path",
                missing.to_str().unwrap(),
                "--cli-version",
                "1.30.0",
            ])
            .unwrap();
        assert!(command.handler(&matches).is_err());
    }
}
