//! Export using the container's freshly built native binary before it starts serving.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::core::{
    application_output::output_path,
    artifact_paths::{self, Artifact},
};

pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn script(scratch: &Path, contract: &Path, openapi: &Path) -> Result<String> {
    let text = |path: &Path| -> Result<String> {
        Ok(quote(path.to_str().context("Export path must be UTF-8")?))
    };
    let contract_parent = contract.parent().context("Contract parent missing")?;
    let openapi_parent = openapi.parent().context("OpenAPI parent missing")?;
    Ok(format!(
        r#"mkdir -p {scratch} && export_dir=$(mktemp -d {scratch}/dev-export-XXXXXX) && trap 'rm -rf "$export_dir"' EXIT HUP INT TERM && FORKLAUNCH_MODE=contract FORKLAUNCH_CONTRACT_OUTPUT="$export_dir/contract.json" FORKLAUNCH_OPENAPI_OUTPUT="$export_dir/openapi.json" "$1" && test -f "$export_dir/contract.json" && test -f "$export_dir/openapi.json" && mkdir -p {contract_parent} {openapi_parent} && (cmp -s "$export_dir/contract.json" {contract} || mv -f "$export_dir/contract.json" {contract}) && (cmp -s "$export_dir/openapi.json" {openapi} || mv -f "$export_dir/openapi.json" {openapi}) && rm -f "$export_dir/contract.json" "$export_dir/openapi.json" && rmdir "$export_dir" && trap - EXIT HUP INT TERM"#,
        scratch = text(scratch)?,
        contract_parent = text(contract_parent)?,
        openapi_parent = text(openapi_parent)?,
        contract = text(contract)?,
        openapi = text(openapi)?
    ))
}

pub(super) fn resolved_output(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(existing.file_name().context("Invalid artifact path")?);
        existing = existing.parent().context("Artifact parent missing")?;
    }
    if !suffix.is_empty() && !existing.is_dir() {
        bail!("Artifact parent must be a directory");
    }
    let mut resolved = existing.canonicalize()?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

/// A project-local Rust SDK whose build script emits the service's contract
/// and OpenAPI artifacts (`build::service_sdks_at`).
fn build_emits_artifacts(root: &Path, project: &toml::Value) -> bool {
    let metadata = project.get("metadata");
    let field = |name: &str| metadata.and_then(|m| m.get(name)).and_then(toml::Value::as_str);
    if field("language") != Some("rust") || field("sdkLayout") != Some("project-local-v1") {
        return false;
    }
    let Some(crate_path) = field("cratePath") else { return false };
    std::fs::read_to_string(root.join(crate_path).join("sdk/rust/build.rs"))
        .is_ok_and(|source| source.contains("service_sdks_at("))
}

/// Shell `forklaunch-native-run` evaluates before each process start.
pub(super) const PRESTART: &str = "FORKLAUNCH_DEV_PRESTART";

enum Startup {
    /// `--no-watch`: the export joins the one-shot build-and-exec entrypoint.
    Entrypoint(String),
    /// Reloads: the export runs before every in-place restart.
    Prestart(String),
}

pub(super) fn enable(
    root: &Path,
    manifest: &toml::Value,
    compose: &mut Value,
    changes: &mut Value,
) -> Result<()> {
    let projects = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .context("Projects missing")?;
    let scratch = resolved_output(&output_path(root, Path::new(".forklaunch"))?)?;
    if scratch.exists() && !scratch.is_dir() {
        bail!("Development export scratch path must be a directory");
    }
    let scratch = Path::new("/app").join(scratch.strip_prefix(root.canonicalize()?)?);
    let mut destinations = BTreeMap::<PathBuf, String>::new();
    let mut plans = Vec::new();
    for (service, change) in changes
        .as_object()
        .context("Native development changes missing")?
    {
        let binary = change["command"][0]
            .as_str()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .context("Native binary missing")?;
        let project = projects
            .iter()
            .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(binary))
            .context("Native project missing")?;
        let metadata = project.get("metadata");
        if build_emits_artifacts(root, project) {
            // Cargo's SDK build script already writes these artifacts on every
            // rebuild. A second, differently serialized startup export would
            // alternate with it, and each alternation would regenerate SDKs and
            // restart the service, forever.
            continue;
        }
        let mut paths = Vec::new();
        for (field, alias, default) in [
            (
                "contractArtifactPath",
                "contract_artifact_path",
                artifact_paths::default_path(
                    binary,
                    metadata.and_then(toml::Value::as_table),
                    Artifact::Contract,
                ),
            ),
            (
                "openapiPath",
                "openapi_path",
                artifact_paths::default_path(
                    binary,
                    metadata.and_then(toml::Value::as_table),
                    Artifact::OpenApi,
                ),
            ),
        ] {
            let configured = metadata
                .and_then(|value| value.get(field).or_else(|| value.get(alias)))
                .and_then(toml::Value::as_str)
                .unwrap_or(&default);
            let destination = resolved_output(&output_path(root, Path::new(configured))?)?;
            if destination.exists() && !destination.is_file() {
                bail!("Artifact destination must be a file");
            }
            let owner = format!("{binary}:{field}");
            if let Some(previous) = destinations.insert(destination.clone(), owner.clone())
                && previous != owner
            {
                bail!("Development artifact collision between {previous} and {owner}");
            }
            paths.push(Path::new("/app").join(destination.strip_prefix(root.canonicalize()?)?));
        }
        let export = script(&scratch, &paths[0], &paths[1])?;
        if change["entrypoint"][0] == "forklaunch-native-run" {
            // Reloading processes export before every in-place restart.
            plans.push((service.clone(), Startup::Prestart(export.replace('$', "$$"))));
            continue;
        }
        let startup = change["entrypoint"][2]
            .as_str()
            .context("Native startup missing")?;
        let build = startup
            .strip_suffix(" && exec \"$@\"")
            .context("Unsupported native startup")?;
        plans.push((
            service.clone(),
            Startup::Entrypoint(format!("{build} && {export} && exec \"$@\"").replace('$', "$$")),
        ));
    }
    for (service, startup) in plans {
        match startup {
            Startup::Entrypoint(script) => {
                compose["services"][&service]["entrypoint"][2] = json!(script);
                changes[&service]["entrypoint"][2] = json!(script);
            }
            Startup::Prestart(script) => {
                compose["services"][&service]["environment"][PRESTART] = json!(script);
                changes[&service]["environment"][PRESTART] = json!(script);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::*;

    #[cfg(unix)]
    #[test]
    fn exports_before_startup_and_preserves_old_outputs_on_export_failure() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let binary = root.path().join("service");
        let contract = root.path().join("quoted ' directory/contract.json");
        let openapi = root.path().join("specs/openapi.json");
        fs::write(
            &binary,
            r#"#!/bin/sh
if [ "$FORKLAUNCH_MODE" = contract ]; then
  printf contract > "$FORKLAUNCH_CONTRACT_OUTPUT"
  test "$FAIL_EXPORT" != 1 || exit 1
  printf openapi > "$FORKLAUNCH_OPENAPI_OUTPUT"
else
  test "$1" = --worker
fi
"#,
        )
        .unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let scratch = root.path().join("scratch");
        let startup = format!(
            "{} && exec \"$@\"",
            script(&scratch, &contract, &openapi).unwrap()
        );
        let run = |fail: &str| {
            Command::new("sh")
                .args(["-c", &startup, "--"])
                .arg(&binary)
                .arg("--worker")
                .env("FAIL_EXPORT", fail)
                .status()
                .unwrap()
        };
        assert!(run("0").success());
        assert_eq!(fs::read_to_string(&contract).unwrap(), "contract");
        assert_eq!(fs::read_to_string(&openapi).unwrap(), "openapi");
        let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(123456789);
        fs::File::options()
            .write(true)
            .open(&contract)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(timestamp))
            .unwrap();
        assert!(run("0").success());
        assert_eq!(
            fs::metadata(&contract).unwrap().modified().unwrap(),
            timestamp
        );
        fs::write(&contract, "old contract").unwrap();
        fs::write(&openapi, "old openapi").unwrap();
        assert!(!run("1").success());
        assert_eq!(fs::read_to_string(contract).unwrap(), "old contract");
        assert_eq!(fs::read_to_string(openapi).unwrap(), "old openapi");
        assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod planning_tests {
    use super::*;
    #[test]
    fn custom_paths_are_read_only_and_collisions_fail_before_plan_mutation() {
        let (root, mut manifest, base) = super::super::tests::fixture();
        manifest["projects"][0]["metadata"]
            .as_table_mut()
            .unwrap()
            .insert(
                "contractArtifactPath".into(),
                toml::Value::String("custom/api.json".into()),
            );
        manifest["projects"][0]["metadata"]
            .as_table_mut()
            .unwrap()
            .insert(
                "openapiPath".into(),
                toml::Value::String("custom/spec.json".into()),
            );
        let (mut planned, mut changes) =
            super::super::plan(root.path(), &manifest, base, &[], true).unwrap();
        let original = planned.clone();
        enable(root.path(), &manifest, &mut planned, &mut changes).unwrap();
        let startup = planned["services"]["native-api"]["entrypoint"][2]
            .as_str()
            .unwrap();
        assert!(startup.contains("/app/custom/api.json"));
        assert!(startup.contains("/app/custom/spec.json"));
        assert!(startup.contains("\"$$export_dir/contract.json\""));
        assert_eq!(
            planned["services"]["legacy-api"],
            original["services"]["legacy-api"]
        );
        assert!(!root.path().join("custom").exists());
        manifest["projects"][0]["metadata"]["openapiPath"] =
            toml::Value::String("custom/api.json".into());
        let before = changes.clone();
        assert!(enable(root.path(), &manifest, &mut planned, &mut changes).is_err());
        assert_eq!(changes, before);
        manifest["projects"][0]["metadata"]["openapiPath"] =
            toml::Value::String("../outside.json".into());
        assert!(enable(root.path(), &manifest, &mut planned, &mut changes).is_err());
    }
}
