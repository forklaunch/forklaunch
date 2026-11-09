//! Refresh portable socket artifacts from native contracts exported by dev containers.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::core::{
    application_output::output_path,
    artifact_paths::{self, Artifact},
    openapi_export::export_registered_sockets,
};

struct Service {
    name: String,
    input: PathBuf,
    output: PathBuf,
    has_sockets: bool,
    metadata: Option<toml::Table>,
    digest: Option<[u8; 32]>,
    error: Option<String>,
}

pub(super) struct Exports {
    root: PathBuf,
    services: Vec<Service>,
    next: Instant,
}

fn destination(
    root: &Path,
    metadata: Option<&toml::Table>,
    field: &str,
    default: String,
) -> Result<PathBuf> {
    let alias = match field {
        "contractArtifactPath" => "contract_artifact_path",
        "openapiPath" => "openapi_path",
        _ => field,
    };
    let relative = metadata
        .and_then(|metadata| metadata.get(field).or_else(|| metadata.get(alias)))
        .and_then(toml::Value::as_str)
        .unwrap_or(&default);
    let path = super::contracts::resolved_output(&output_path(root, Path::new(relative))?)?;
    ensure!(
        !path.exists() || path.is_file(),
        "Development artifact must be a file"
    );
    Ok(path)
}

impl Exports {
    pub(super) fn new(root: &Path, manifest: &toml::Value, native: &Value) -> Result<Self> {
        let root = root.canonicalize()?;
        let names: BTreeSet<_> = native
            .as_object()
            .context("Native changes missing")?
            .values()
            .filter_map(|change| change["command"][0].as_str())
            .filter_map(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
            .collect();
        let mut services = Vec::new();
        let mut occupied = BTreeMap::new();
        let mut outputs = Vec::new();
        if !names.is_empty() {
            for project in manifest
                .get("projects")
                .and_then(toml::Value::as_array)
                .context("Projects missing")?
            {
                let Some(name) = project.get("name").and_then(toml::Value::as_str) else {
                    continue;
                };
                if !matches!(
                    project.get("type").and_then(toml::Value::as_str),
                    Some("Service" | "Worker")
                ) {
                    continue;
                }
                let metadata = project
                    .get("metadata")
                    .and_then(toml::Value::as_table)
                    .cloned();
                let contract = destination(
                    &root,
                    metadata.as_ref(),
                    "contractArtifactPath",
                    artifact_paths::default_path(name, metadata.as_ref(), Artifact::Contract),
                )?;
                let socket = destination(
                    &root,
                    metadata.as_ref(),
                    "socketContractArtifactPath",
                    artifact_paths::default_path(name, metadata.as_ref(), Artifact::SocketContract),
                )?;
                let marker = destination(
                    &root,
                    None,
                    "marker",
                    artifact_paths::socket_registry_marker(name, metadata.as_ref())
                        .to_string_lossy()
                        .into_owned(),
                )?;
                for (kind, path) in [
                    ("contract", contract.clone()),
                    (
                        "OpenAPI",
                        destination(
                            &root,
                            metadata.as_ref(),
                            "openapiPath",
                            artifact_paths::default_path(name, metadata.as_ref(), Artifact::OpenApi),
                        )?,
                    ),
                    ("socket", socket.clone()),
                    ("socket ownership", marker.clone()),
                ] {
                    occupied
                        .entry(path)
                        .or_insert_with(Vec::new)
                        .push(format!("{name}:{kind}"));
                }
                if names.contains(name) {
                    outputs.extend([socket.clone(), marker]);
                    services.push(Service {
                        name: name.into(),
                        input: contract,
                        output: socket,
                        has_sockets: false,
                        metadata,
                        digest: None,
                        error: None,
                    });
                }
            }
            for output in outputs {
                let owners = &occupied[&output];
                ensure!(
                    owners.len() == 1,
                    "Development socket artifact collision: {}",
                    owners.join(", ")
                );
            }
        }
        Ok(Self {
            root,
            services,
            next: Instant::now(),
        })
    }

    pub(super) fn poll(&mut self) {
        if Instant::now() < self.next {
            return;
        }
        self.next = Instant::now() + Duration::from_secs(1);
        self.refresh();
    }

    pub(super) fn prepare(&mut self) -> Result<()> {
        for service in &mut self.services {
            refresh(&self.root, service).with_context(|| {
                format!("Unable to prepare development sockets for {}", service.name)
            })?;
        }
        Ok(())
    }

    fn refresh(&mut self) {
        for service in &mut self.services {
            let result = refresh(&self.root, service);
            match result {
                Ok(()) => service.error = None,
                Err(error) => {
                    let message = format!("{error:#}");
                    if service.error.as_ref() != Some(&message) {
                        eprintln!(
                            "Socket export for {} failed; waiting to retry: {message}",
                            service.name
                        );
                        service.error = Some(message);
                    }
                }
            }
        }
    }
}

fn refresh(root: &Path, service: &mut Service) -> Result<()> {
    let file = match fs::File::open(&service.input) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Native contract exceeds 16 MiB"
    );
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if service.digest == Some(digest) && (!service.has_sockets || service.output.is_file()) {
        return Ok(());
    }
    let artifact: Value = serde_json::from_slice(&bytes).context("Invalid native contract")?;
    ensure!(
        artifact["generator"] == "framework-rs"
            && artifact["formatVersion"] == "forklaunch.contract.v1",
        "Unsupported native contract format"
    );
    ensure!(
        artifact["service"]["name"].is_string()
            && artifact["service"]["version"].is_string()
            && artifact["routes"].is_array(),
        "Incomplete native contract"
    );
    export_registered_sockets(root, &service.name, service.metadata.as_ref(), &artifact)?;
    service.has_sockets = artifact
        .get("sockets")
        .and_then(Value::as_object)
        .is_some_and(|sockets| !sockets.is_empty());
    service.digest = Some(digest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn follows_native_exports_preserves_failed_exports_and_retires_owned_sockets() {
        let root = tempfile::tempdir().unwrap();
        let manifest: toml::Value = toml::from_str("[[projects]]\nname='api'\ntype='Service'\n[projects.metadata]\nlanguage='rust'\ncontractArtifactPath='custom/http.json'\nsocketContractArtifactPath='custom/socket.json'\n").unwrap();
        let native = json!({"api":{"command":["/app/target/debug/api"]},"api-worker":{"command":["/app/target/debug/api","--worker"]}});
        let mut exports = Exports::new(root.path(), &manifest, &native).unwrap();
        assert_eq!(exports.services.len(), 1);
        assert!(!root.path().join("custom").exists());
        exports.refresh();
        assert!(exports.services[0].error.is_none());
        fs::create_dir(root.path().join("custom")).unwrap();
        let input = root.path().join("custom/http.json");
        let output = root.path().join("custom/socket.json");
        let mut artifact = json!({"generator":"framework-rs","formatVersion":"forklaunch.contract.v1","service":{"name":"API","version":"1.0"},"routes":[],"sockets":{"chat":{"path":"/chat"}}});
        fs::write(&input, artifact.to_string()).unwrap();
        exports.refresh();
        let previous = fs::read(&output).unwrap();
        let modified = fs::metadata(&output).unwrap().modified().unwrap();
        exports.refresh();
        assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
        fs::remove_file(&output).unwrap();
        exports.refresh();
        assert_eq!(fs::read(&output).unwrap(), previous);
        let modified = fs::metadata(&output).unwrap().modified().unwrap();
        artifact["developmentProbe"] = json!(true);
        fs::write(&input, artifact.to_string()).unwrap();
        exports.refresh();
        assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
        fs::write(&input, "{partial").unwrap();
        exports.refresh();
        assert!(exports.services[0].error.is_some());
        assert_eq!(fs::read(&output).unwrap(), previous);
        artifact["sockets"] = json!({});
        fs::write(&input, artifact.to_string()).unwrap();
        exports.refresh();
        assert!(exports.services[0].error.is_none());
        assert!(!output.exists());
        artifact["sockets"] = json!({"chat":{"path":"/returned"}});
        fs::write(&input, artifact.to_string()).unwrap();
        exports.refresh();
        assert!(fs::read_to_string(&output).unwrap().contains("/returned"));
        fs::write(&output, "manually edited").unwrap();
        artifact["sockets"] = json!({});
        fs::write(&input, artifact.to_string()).unwrap();
        exports.refresh();
        assert_eq!(fs::read_to_string(&output).unwrap(), "manually edited");
    }

    #[test]
    fn rejects_collisions_with_unselected_service_artifacts_before_writes() {
        let root = tempfile::tempdir().unwrap();
        let manifest: toml::Value = toml::from_str("[[projects]]\nname='api'\ntype='Service'\n[projects.metadata]\nsocketContractArtifactPath='shared.json'\n[[projects]]\nname='other'\ntype='Service'\n[projects.metadata]\nopenapiPath='shared.json'\n").unwrap();
        assert!(
            Exports::new(
                root.path(),
                &manifest,
                &json!({"api":{"command":["/app/target/debug/api"]}})
            )
            .is_err()
        );
        assert!(!root.path().join("shared.json").exists());
    }
}
