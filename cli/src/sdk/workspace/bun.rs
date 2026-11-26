//! Preserve the scaffold's portable Bun install policy when moving installation.
use std::{fs, path::Path};

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table};

use crate::core::application_output::output_path;

pub(super) fn include_packages(
    package: &mut serde_json::Map<String, serde_json::Value>,
    additions: &[String],
    absent_generated_members: &[String],
) -> Result<()> {
    let workspaces = package
        .entry("workspaces")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("Bun workspaces must be a list")?;
    // Bun rejects absent literal package members. Only these known optional
    // generated packages may be removed; authored members and globs stay.
    workspaces.retain(|member| {
        !member.as_str().is_some_and(|path| {
            absent_generated_members
                .iter()
                .any(|missing| missing == path)
        })
    });
    for addition in additions {
        if absent_generated_members.contains(addition) {
            continue;
        }
        let value = serde_json::json!(addition);
        if !workspaces.contains(&value) {
            workspaces.push(value);
        }
    }
    Ok(())
}

pub(super) fn install_policy(root: &Path, modules: &Path) -> Result<Option<String>> {
    let source = output_path(root, &modules.join("bunfig.toml"))?;
    if !source.is_file() {
        return Ok(None);
    }
    let source: DocumentMut = fs::read_to_string(source)?.parse()?;
    let Some(install) = source.get("install") else {
        return Ok(None);
    };
    let install = install
        .as_table_like()
        .context("Module Bun install settings must be a table")?;
    // These generated settings are portable. Other Bun options may reference
    // files relative to the modules directory and must not be copied blindly.
    let defaults: Vec<_> = ["minimumReleaseAge", "minimumReleaseAgeExcludes"]
        .into_iter()
        .filter_map(|name| install.get(name).map(|value| (name, value.clone())))
        .collect();
    if defaults.is_empty() {
        return Ok(None);
    }
    let destination = output_path(root, Path::new("bunfig.toml"))?;
    let mut document: DocumentMut = if destination.exists() {
        fs::read_to_string(destination)?.parse()?
    } else {
        DocumentMut::new()
    };
    let root_install = document
        .entry("install")
        .or_insert(Item::Table(Table::new()))
        .as_table_like_mut()
        .context("Root Bun install settings must be a table")?;
    for (name, value) in defaults {
        if !root_install.contains_key(name) {
            root_install.insert(name, value);
        }
    }
    Ok(Some(document.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_portable_policy_without_writes_and_preserves_root_overrides() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("modules")).unwrap();
        fs::write(root.path().join("modules/bunfig.toml"), "[install]\nminimumReleaseAge = 86400\nminimumReleaseAgeExcludes = ['@forklaunch/core']\ncafile = './module-ca.pem'\n").unwrap();
        let generated = install_policy(root.path(), Path::new("modules"))
            .unwrap()
            .unwrap();
        let policy: toml::Value = toml::from_str(&generated).unwrap();
        assert_eq!(
            policy["install"]["minimumReleaseAge"].as_integer(),
            Some(86400)
        );
        assert!(policy["install"].get("cafile").is_none());
        assert!(!root.path().join("bunfig.toml").exists());
        let original =
            "# application owned\n[install]\nminimumReleaseAge = 172800\n[run]\nshell = 'system'\n";
        fs::write(root.path().join("bunfig.toml"), original).unwrap();
        let merged = install_policy(root.path(), Path::new("modules"))
            .unwrap()
            .unwrap();
        assert!(merged.starts_with("# application owned\n"));
        let policy: toml::Value = toml::from_str(&merged).unwrap();
        assert_eq!(
            policy["install"]["minimumReleaseAge"].as_integer(),
            Some(172800)
        );
        assert_eq!(
            policy["install"]["minimumReleaseAgeExcludes"][0].as_str(),
            Some("@forklaunch/core")
        );
        assert_eq!(policy["run"]["shell"].as_str(), Some("system"));
        assert_eq!(
            fs::read_to_string(root.path().join("bunfig.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn missing_policy_is_optional_and_external_destinations_reject() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("modules")).unwrap();
        assert!(
            install_policy(root.path(), Path::new("modules"))
                .unwrap()
                .is_none()
        );
        fs::write(
            root.path().join("modules/bunfig.toml"),
            "[install]\nminimumReleaseAge = 86400\n",
        )
        .unwrap();
        fs::write(root.path().join("bunfig.toml"), "install = false\n").unwrap();
        assert!(install_policy(root.path(), Path::new("modules")).is_err());
        #[cfg(unix)]
        {
            let external = tempfile::NamedTempFile::new().unwrap();
            fs::remove_file(root.path().join("bunfig.toml")).unwrap();
            std::os::unix::fs::symlink(external.path(), root.path().join("bunfig.toml")).unwrap();
            assert!(install_policy(root.path(), Path::new("modules")).is_err());
        }
    }
}
