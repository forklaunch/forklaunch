//! Keep registered OpenAPI artifacts current while retaining the requested export layout.
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::core::application_output::output_path;

pub(super) struct Outputs(BTreeMap<String, (PathBuf, PathBuf)>);

/// Keep compiler-owned bytes and provenance when an export only changes JSON
/// formatting. All writes still use the generated-output safety checks.
pub(super) fn write_json(path: &Path, document: &serde_json::Value) -> Result<()> {
    let existing = fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .and_then(|_| fs::read(path).ok())
        .filter(|bytes| {
            serde_json::from_slice::<serde_json::Value>(bytes)
                .is_ok_and(|existing| existing == *document)
        });
    let bytes = match existing {
        Some(bytes) => bytes,
        None => serde_json::to_vec_pretty(document)?,
    };
    forklaunch_core::build::write_generated_output(path, bytes)
}

fn identity(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut suffix = Vec::new();
    while !existing.exists() {
        if fs::symlink_metadata(existing).is_ok() {
            bail!("OpenAPI output contains a dangling symlink");
        }
        suffix.push(existing.file_name().context("Invalid OpenAPI output")?);
        existing = existing.parent().context("OpenAPI output parent missing")?;
    }
    let mut resolved = existing.canonicalize()?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

impl Outputs {
    /// `directory` is an explicitly requested export folder (`<dir>/<name>/openapi.json`).
    /// Without one, each service exports straight to its registered artifact,
    /// or to the legacy `.forklaunch/openapi/<name>/openapi.json` when it has none.
    pub(super) fn plan<'a>(
        root: &Path,
        directory: Option<&Path>,
        projects: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) -> Result<Self> {
        let canonical_root = root.canonicalize()?;
        let mut outputs = BTreeMap::new();
        let mut owners = BTreeMap::new();
        for (name, configured) in projects {
            let mut components = Path::new(name).components();
            if !matches!(components.next(), Some(Component::Normal(_)))
                || components.next().is_some()
                || name.contains('\\')
            {
                bail!("OpenAPI service name must be one path component");
            }
            let requested = directory.map(|directory| directory.join(name).join("openapi.json"));
            let registered = if let Some(configured) = configured {
                let path = Path::new(configured);
                let relative = if path.is_absolute() {
                    path.strip_prefix(&canonical_root)
                        .or_else(|_| path.strip_prefix(root))
                        .context("OpenAPI artifact must stay inside the application")?
                } else {
                    path
                };
                output_path(root, relative)?
            } else if let Some(requested) = &requested {
                requested.clone()
            } else {
                output_path(
                    root,
                    Path::new(&crate::core::artifact_paths::legacy(
                        name,
                        crate::core::artifact_paths::Artifact::OpenApi,
                    )),
                )?
            };
            let exported = requested.unwrap_or_else(|| registered.clone());
            for path in [&exported, &registered] {
                if path.exists() && !path.is_file() {
                    bail!("OpenAPI output must be a file: {}", path.display());
                }
                let resolved = identity(path)?;
                if let Some(previous) = owners.insert(resolved, name.to_owned())
                    && previous != name
                {
                    bail!("OpenAPI output collision between {previous} and {name}");
                }
            }
            outputs.insert(name.to_owned(), (exported, registered));
        }
        Ok(Self(outputs))
    }

    /// Where the exporter writes `name`'s document before registered copies refresh.
    pub(super) fn exported(&self, name: &str) -> Result<&Path> {
        Ok(&self.0.get(name).context("Missing OpenAPI output plan")?.0)
    }

    pub(super) fn refresh(&self, name: &str) -> Result<()> {
        let (exported, registered) = self.0.get(name).context("Missing OpenAPI output plan")?;
        if identity(exported)? == identity(registered)? {
            return Ok(());
        }
        let document = serde_json::from_slice(&fs::read(exported)?)
            .context("Invalid exported OpenAPI document")?;
        write_json(registered, &document)
            .with_context(|| format!("Failed to refresh {}", registered.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_artifact_refresh_preserves_requested_export_layout() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("release-openapi");
        let registered = root.path().join("schemas/api.json");
        let plan = Outputs::plan(root.path(), Some(&directory), [("api", registered.to_str())]).unwrap();
        fs::create_dir_all(directory.join("api")).unwrap();
        let fresh = serde_json::json!({"openapi":"3.1.0","info":{"title":"fresh"}});
        fs::write(directory.join("api/openapi.json"), fresh.to_string()).unwrap();
        plan.refresh("api").unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&registered).unwrap()).unwrap(),
            fresh
        );
        assert_eq!(
            fs::read_to_string(directory.join("api/openapi.json")).unwrap(),
            fresh.to_string()
        );
        fs::write(&registered, "stale").unwrap();
        plan.refresh("api").unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(registered).unwrap()).unwrap(),
            fresh
        );
    }

    #[test]
    fn default_export_writes_registered_artifacts_and_legacy_only_without_one() {
        let root = tempfile::tempdir().unwrap();
        let plan = Outputs::plan(
            root.path(),
            None,
            [("api", Some("src/api/sdk/typescript/openapi.json")), ("legacy", None)],
        )
        .unwrap();
        let api = plan.exported("api").unwrap();
        assert!(api.ends_with("src/api/sdk/typescript/openapi.json"));
        assert!(plan.exported("legacy").unwrap().ends_with(".forklaunch/openapi/legacy/openapi.json"));
        fs::create_dir_all(api.parent().unwrap()).unwrap();
        fs::write(api, "{}").unwrap();
        plan.refresh("api").unwrap();
        assert!(!root.path().join(".forklaunch").exists());
    }

    #[test]
    fn identical_json_export_preserves_compiler_bytes_timestamps_and_provenance() {
        use forklaunch_core::{
            build::{
                http_artifact_sources,
                provenance::{BuildContext, HttpOutputs, InputScope, InputSnapshot, record_http},
            },
            contract::{BodyContract, ContractRegistry, RouteContract},
        };
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("sdk")).unwrap();
        fs::write(root.path().join("source.rs"), "// declared contract input").unwrap();
        let mut registry = ContractRegistry::default();
        registry.register(
            RouteContract::get("get", "Get", "/item")
                .with_response(200, Some(BodyContract::json("String"))),
        );
        let mut contract: serde_json::Value =
            serde_json::from_str(&registry.clone().into_artifact().to_json()).unwrap();
        contract["rustClientName"] = "ItemsClient".into();
        contract["rustOutputLayout"] = "service-module".into();
        let source = contract.to_string();
        let compiled = http_artifact_sources(&source, "items").unwrap();
        let contract_path = root.path().join("sdk/contract.json");
        let openapi_path = root.path().join("sdk/openapi.json");
        fs::write(&contract_path, source).unwrap();
        fs::write(root.path().join("sdk/index.ts"), compiled.typescript).unwrap();
        fs::write(root.path().join("sdk/client.rs"), compiled.rust).unwrap();
        fs::write(
            &openapi_path,
            forklaunch_core::openapi::generate_portable_openapi_json(&registry).unwrap(),
        )
        .unwrap();
        let context = BuildContext {
            host: "fixture-host".into(),
            target: "fixture-target".into(),
            features: Vec::new(),
            configuration: BTreeMap::new(),
        };
        let receipt = record_http(
            root.path(),
            InputSnapshot::capture(root.path(), &[InputScope::required("source.rs")]).unwrap(),
            context.clone(),
            "items",
            HttpOutputs {
                contract: "sdk/contract.json".into(),
                typescript: "sdk/index.ts".into(),
                rust: "sdk/client.rs".into(),
                additional: vec!["sdk/openapi.json".into()],
            },
        )
        .unwrap();
        receipt.verify(root.path(), &context).unwrap();
        let original = [&contract_path, &openapi_path].map(|path| {
            (
                fs::read(path).unwrap(),
                fs::metadata(path).unwrap().modified().unwrap(),
            )
        });
        let output = root.path().join("exports");
        let plan =
            Outputs::plan(root.path(), Some(&output), [("items", Some("sdk/openapi.json"))]).unwrap();
        let mut openapi: serde_json::Value = serde_json::from_slice(&original[1].0).unwrap();
        write_json(&output.join("items/openapi.json"), &openapi).unwrap();
        write_json(&contract_path, &contract).unwrap();
        plan.refresh("items").unwrap();
        for (path, (bytes, modified)) in [&contract_path, &openapi_path].into_iter().zip(original) {
            assert_eq!(fs::read(path).unwrap(), bytes);
            assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
        }
        receipt.verify(root.path(), &context).unwrap();
        // Genuine changes still publish and invalidate the prior compiler receipt.
        openapi["info"]["title"] = "Changed".into();
        write_json(&output.join("items/openapi.json"), &openapi).unwrap();
        plan.refresh("items").unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(openapi_path).unwrap()).unwrap(),
            openapi
        );
        assert!(receipt.verify(root.path(), &context).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn equivalent_json_does_not_bypass_generated_output_file_checks() {
        let root = tempfile::tempdir().unwrap();
        let document = serde_json::json!({"openapi":"3.1.0"});
        let target = root.path().join("target.json");
        fs::write(&target, document.to_string()).unwrap();
        let alias = root.path().join("alias.json");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(write_json(&alias, &document).is_err());
        assert!(
            fs::symlink_metadata(alias)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(target).unwrap(), document.to_string());
        assert!(write_json(root.path(), &document).is_err());
    }

    #[test]
    fn output_preflight_rejects_escape_and_cross_service_collisions() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(".forklaunch/openapi");
        for path in ["../outside.json", "new/../../outside.json", ""] {
            assert!(Outputs::plan(root.path(), Some(&directory), [("api", Some(path))]).is_err());
        }
        assert!(
            Outputs::plan(
                root.path(),
                Some(&directory),
                [("api", Some("same.json")), ("other", Some("same.json"))]
            )
            .is_err()
        );
        assert!(
            Outputs::plan(
                root.path(),
                Some(&directory),
                [
                    ("api", Some(".forklaunch/openapi/other/openapi.json")),
                    ("other", None)
                ]
            )
            .is_err()
        );
        assert!(Outputs::plan(root.path(), Some(&directory), [("../api", None)]).is_err());
        assert!(!directory.exists());
    }

    #[cfg(unix)]
    #[test]
    fn output_preflight_checks_symlinks_and_refresh_preserves_same_file_aliases() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let directory = root.path().join("exports");
        symlink(external.path(), root.path().join("outside")).unwrap();
        symlink(root.path().join("missing"), root.path().join("dangling")).unwrap();
        for path in ["outside/api.json", "dangling"] {
            assert!(Outputs::plan(root.path(), Some(&directory), [("api", Some(path))]).is_err());
        }
        fs::create_dir_all(directory.join("api")).unwrap();
        let exported = directory.join("api/openapi.json");
        fs::write(&exported, "original").unwrap();
        symlink(&exported, root.path().join("alias.json")).unwrap();
        let plan = Outputs::plan(root.path(), Some(&directory), [("api", Some("alias.json"))]).unwrap();
        plan.refresh("api").unwrap();
        assert_eq!(fs::read_to_string(exported).unwrap(), "original");
    }
}
