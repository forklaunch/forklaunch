//! Remember imported TypeScript services so a missing export cannot silently
//! remove them from the shared SDK during an unrelated Rust lifecycle update.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::core::{
    artifact_paths::{self, Artifact},
    forklaunch_state::MANIFEST,
};

use anyhow::{Context, Result, bail};

/// TypeScript services whose SDKs were already generated, read from the
/// generated Rust SDK crates on disk. A service listed here must keep its
/// OpenAPI export; losing it is an error rather than a silent removal.
pub(super) fn read(root: &Path) -> Result<BTreeSet<String>> {
    let Ok(source) = fs::read_to_string(root.join(MANIFEST)) else {
        return Ok(BTreeSet::new());
    };
    let manifest: toml::Value =
        toml::from_str(&source).context("Failed to read the application manifest")?;
    let shared = root.join(super::layout::rust(root)?);
    let typescript = root.join(super::layout::typescript(root)?);
    let recorded: BTreeSet<String> = fs::read_to_string(super::layout::source(root, &typescript)?.join(RECORD))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let mut services = BTreeSet::new();
    for project in manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|project| is_typescript(project))
    {
        let Some(name) = project.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let local = super::layout::local_sdk(root, name, "rust")?;
        if shared.join(format!("{name}-sdk/Cargo.toml")).is_file()
            || local.is_some_and(|directory| directory.join("Cargo.toml").is_file())
            || recorded.contains(name)
        {
            services.insert(name.to_owned());
        }
    }
    Ok(services)
}

/// Services written into the shared TypeScript SDK, beside its index.
const RECORD: &str = ".forklaunch-services";

pub(super) fn record<'a>(source: &Path, names: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut names: Vec<&str> = names.collect();
    names.sort_unstable();
    names.dedup();
    super::write_if_changed(source.join(RECORD), names.iter().map(|name| format!("{name}\n")).collect::<String>())?;
    Ok(())
}

pub(super) fn is_typescript(project: &toml::Value) -> bool {
    project
        .get("metadata")
        .and_then(|metadata| metadata.get("language"))
        .and_then(toml::Value::as_str)
        != Some("rust")
}

pub(super) fn projects<'a>(
    manifest: &'a toml::Value,
    selected: Option<&str>,
) -> Vec<&'a toml::Value> {
    manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|project| {
            crate::core::openapi_export::raw_contract_export_enabled(project)
                && matches!(
                    project.get("type").and_then(toml::Value::as_str),
                    Some("Service" | "Worker")
                )
                && matches!(
                    project
                        .get("metadata")
                        .and_then(|metadata| metadata.get("language"))
                        .and_then(toml::Value::as_str),
                    None | Some("rust" | "typescript" | "ts")
                )
                && project
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| {
                        selected.is_none_or(|selected| selected == name)
                            && (!is_typescript(project)
                                || !crate::core::openapi_export::OBSERVABILITY_SERVICES
                                    .contains(&name))
                    })
        })
        .collect()
}

pub(super) fn artifact(
    root: &Path,
    project: &toml::Value,
    source: &str,
) -> Result<(&'static str, PathBuf)> {
    let name = project
        .get("name")
        .and_then(toml::Value::as_str)
        .context("SDK service name is required")?;
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
        || name.contains('\\')
    {
        bail!("SDK service name must be one path component");
    }
    let explicit_contract = project
        .get("metadata")
        .and_then(|metadata| metadata.get("contractArtifactPath"))
        .and_then(toml::Value::as_str)
        .is_some();
    let (source, artifact) =
        if source == "openapi" || (is_typescript(project) && !explicit_contract) {
            ("openapi", Artifact::OpenApi)
        } else {
            ("contract", Artifact::Contract)
        };
    let root = root.canonicalize()?;
    let path = root.join(
        artifact_paths::project_path(project, artifact).context("SDK service name is required")?,
    );
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        if fs::symlink_metadata(ancestor).is_ok() {
            bail!("SDK artifact contains a dangling symlink");
        }
        ancestor = ancestor
            .parent()
            .context("SDK artifact parent is missing")?;
    }
    if !ancestor.canonicalize()?.starts_with(&root) {
        bail!("SDK artifact path must stay inside the application");
    }
    Ok((source, path))
}

pub(super) fn watched_paths(
    root: &Path,
    source: &str,
    selected: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(&manifest_path)?)?;
    let mut paths = vec![manifest_path];
    for project in projects(&manifest, selected) {
        paths.push(artifact(root, project, source)?.1);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_explicit_portable_contract_overrides_legacy_default() {
        let root = tempfile::tempdir().unwrap();
        let project: toml::Value = toml::from_str(
            r#"
name = "notes"
type = "Service"
[metadata]
language = "typescript"
contractArtifactPath = ".forklaunch/contracts/notes/contract.json"
"#,
        )
        .unwrap();
        let (source, path) = artifact(root.path(), &project, "contract").unwrap();
        assert_eq!(source, "contract");
        assert!(path.ends_with("contracts/notes/contract.json"));
        assert_eq!(
            artifact(root.path(), &project, "openapi").unwrap().0,
            "openapi"
        );
        let legacy: toml::Value = toml::from_str("name = 'notes'\ntype = 'Service'").unwrap();
        assert_eq!(
            artifact(root.path(), &legacy, "contract").unwrap().0,
            "openapi"
        );
    }
}
