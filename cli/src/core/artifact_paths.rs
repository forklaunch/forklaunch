//! Default locations for per-project generated artifacts.
//!
//! A generated application's `.forklaunch/` holds only `manifest.toml`. Each
//! service keeps its contract, OpenAPI, AsyncAPI and socket artifacts in its own
//! SDK folder (`<crate>/sdk/typescript/` for Rust, `<staticPath>/sdk/` for
//! TypeScript-authored SDKs, both `sdkLayout = "project-local-v1"`). Explicit
//! manifest paths always win, and projects without a service-owned SDK keep the
//! `.forklaunch/<kind>/<name>/` location TypeScript applications already use.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Artifact {
    Contract,
    OpenApi,
    AsyncApi,
    SocketContract,
}

impl Artifact {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Contract => "contractArtifactPath",
            Self::OpenApi => "openapiPath",
            Self::AsyncApi => "asyncapiPath",
            Self::SocketContract => "socketContractArtifactPath",
        }
    }

    fn alias(self) -> &'static str {
        match self {
            Self::Contract => "contract_artifact_path",
            Self::OpenApi => "openapi_path",
            Self::AsyncApi => "asyncapi_path",
            Self::SocketContract => "socket_contract_artifact_path",
        }
    }

    pub(crate) fn file(self) -> &'static str {
        match self {
            Self::Contract => "contract.json",
            Self::OpenApi => "openapi.json",
            Self::AsyncApi => "asyncapi.json",
            Self::SocketContract => "socket-contract.json",
        }
    }

    fn legacy_directory(self) -> &'static str {
        match self {
            Self::Contract => "contracts",
            Self::OpenApi => "openapi",
            Self::AsyncApi => "asyncapi",
            Self::SocketContract => "socket-contracts",
        }
    }
}

/// The pre-service-SDK location, still read for applications that used it.
pub(crate) fn legacy(name: &str, artifact: Artifact) -> String {
    format!(
        ".forklaunch/{}/{name}/{}",
        artifact.legacy_directory(),
        artifact.file()
    )
}

fn text<'a>(metadata: &'a toml::Table, key: &str) -> Option<&'a str> {
    metadata.get(key).and_then(toml::Value::as_str)
}

fn explicit(metadata: &toml::Table, artifact: Artifact) -> Option<&str> {
    text(metadata, artifact.key()).or_else(|| text(metadata, artifact.alias()))
}

/// The folder that owns a project's generated SDK artifacts, when it has one.
pub(crate) fn sdk_directory(metadata: &toml::Table) -> Option<String> {
    if text(metadata, "sdkLayout") != Some("project-local-v1") {
        return None;
    }
    let project = text(metadata, "cratePath")
        .or_else(|| text(metadata, "crate_path"))
        .or_else(|| text(metadata, "staticPath"))
        .or_else(|| text(metadata, "static_path"))?
        .trim_end_matches('/');
    Some(if text(metadata, "language") == Some("rust") {
        format!("{project}/sdk/typescript")
    } else {
        format!("{project}/sdk")
    })
}

/// The default for `artifact` when the manifest does not name one explicitly.
pub(crate) fn default_path(
    name: &str,
    metadata: Option<&toml::Table>,
    artifact: Artifact,
) -> String {
    metadata.and_then(sdk_directory).map_or_else(
        || legacy(name, artifact),
        |directory| format!("{directory}/{}", artifact.file()),
    )
}

/// The configured path for `artifact`, falling back to the project default.
pub(crate) fn path(name: &str, metadata: Option<&toml::Table>, artifact: Artifact) -> String {
    metadata
        .and_then(|metadata| explicit(metadata, artifact))
        .map_or_else(|| default_path(name, metadata, artifact), str::to_owned)
}

/// [`path`] for a raw manifest project entry.
pub(crate) fn project_path(project: &toml::Value, artifact: Artifact) -> Option<String> {
    let name = project.get("name").and_then(toml::Value::as_str)?;
    Some(path(
        name,
        project.get("metadata").and_then(toml::Value::as_table),
        artifact,
    ))
}

/// [`path`] for a project looked up by name in a raw manifest.
pub(crate) fn manifest_path(manifest: &toml::Value, name: &str, artifact: Artifact) -> String {
    let metadata = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .and_then(|projects| {
            projects
                .iter()
                .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(name))
        })
        .and_then(|project| project.get("metadata"))
        .and_then(toml::Value::as_table);
    path(name, metadata, artifact)
}

/// [`manifest_path`] reading `<root>/.forklaunch/manifest.toml`; an unreadable
/// manifest falls back to the legacy location.
pub(crate) fn application_path(root: &Path, name: &str, artifact: Artifact) -> String {
    std::fs::read_to_string(root.join(".forklaunch/manifest.toml"))
        .ok()
        .and_then(|content| toml::from_str::<toml::Value>(&content).ok())
        .map_or_else(
            || legacy(name, artifact),
            |manifest| manifest_path(&manifest, name, artifact),
        )
}

/// Records which socket artifact the last registry export wrote. It sits beside
/// the project's socket artifact rather than in the application's `.forklaunch/`.
pub(crate) fn socket_registry_marker(name: &str, metadata: Option<&toml::Table>) -> PathBuf {
    match metadata.and_then(sdk_directory) {
        Some(directory) => PathBuf::from(directory).join("socket-registry-export.json"),
        None => PathBuf::from(format!(
            ".forklaunch/socket-contracts/{name}/registry-export.json"
        )),
    }
}

/// Ownership hashes for one generated socket target (`typescript`, `rust` or
/// `asyncapi`), kept in the project's SDK folder when it has one.
pub(crate) fn socket_receipt(name: &str, metadata: Option<&toml::Table>, target: &str) -> PathBuf {
    match metadata.and_then(sdk_directory) {
        Some(directory) => PathBuf::from(directory).join(format!("socket-sdk-{target}.json")),
        None => PathBuf::from(format!(
            ".forklaunch/socket-sdk-outputs/{name}/{target}.json"
        )),
    }
}

/// Raw project metadata for `name` in `<root>/.forklaunch/manifest.toml`.
pub(crate) fn application_metadata(root: &Path, name: &str) -> Option<toml::Table> {
    let manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(root.join(".forklaunch/manifest.toml")).ok()?)
            .ok()?;
    manifest
        .get("projects")?
        .as_array()?
        .iter()
        .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(name))?
        .get("metadata")?
        .as_table()
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(source: &str) -> toml::Table {
        toml::from_str(source).unwrap()
    }

    #[test]
    fn service_owned_sdks_hold_every_artifact() {
        let rust = table("sdkLayout='project-local-v1'\nlanguage='rust'\ncratePath='src/notes'\n");
        let typescript = table(
            "sdkLayout='project-local-v1'\nlanguage='typescript'\nstaticPath='modules/legacy'\n",
        );
        for (artifact, file) in [
            (Artifact::Contract, "contract.json"),
            (Artifact::OpenApi, "openapi.json"),
            (Artifact::AsyncApi, "asyncapi.json"),
            (Artifact::SocketContract, "socket-contract.json"),
        ] {
            assert_eq!(
                path("notes", Some(&rust), artifact),
                format!("src/notes/sdk/typescript/{file}")
            );
            assert_eq!(
                path("legacy", Some(&typescript), artifact),
                format!("modules/legacy/sdk/{file}")
            );
        }
    }

    #[test]
    fn explicit_and_registered_paths_win_and_unowned_projects_keep_legacy_paths() {
        let custom = table(
            "asyncapiPath='docs/events.json'\ncontractArtifactPath='src/client-sdk/typescript/notes/contract.json'\n",
        );
        assert_eq!(
            path("notes", Some(&custom), Artifact::AsyncApi),
            "docs/events.json"
        );
        assert_eq!(
            path("notes", Some(&custom), Artifact::SocketContract),
            ".forklaunch/socket-contracts/notes/socket-contract.json"
        );
        assert_eq!(
            path("notes", None, Artifact::OpenApi),
            ".forklaunch/openapi/notes/openapi.json"
        );
        assert_eq!(
            socket_registry_marker("notes", None),
            Path::new(".forklaunch/socket-contracts/notes/registry-export.json")
        );
        let rust = table("sdkLayout='project-local-v1'\nlanguage='rust'\ncratePath='src/notes'\n");
        assert_eq!(
            socket_receipt("notes", Some(&rust), "rust"),
            Path::new("src/notes/sdk/typescript/socket-sdk-rust.json")
        );
    }
}
