//! Content ownership lets regeneration prune obsolete modules without deleting edits.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;
use crate::core::artifact_paths::{self, Artifact};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ownership {
    format_version: u8,
    #[serde(default)]
    registry_exported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    files: BTreeMap<PathBuf, String>,
}

#[cfg(test)]
fn package(name: &str, target: &str) -> PathBuf {
    match target {
        "rust" => PathBuf::from(format!("sdks/rust/{name}-websocket-sdk")),
        "typescript" => PathBuf::from(format!("sdks/typescript-websocket/{name}")),
        _ => unreachable!("validated SDK target"),
    }
}

fn package_at(root: &Path, name: &str, target: &str) -> Result<PathBuf> {
    let base = crate::sdk::sdk_base(root)?;
    Ok(match target {
        "rust" => base.join(format!("rust/{name}-websocket-sdk")),
        "typescript" => base.join(format!("typescript-websocket/{name}")),
        _ => unreachable!("validated SDK target"),
    })
}

fn hash(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// Aliases are user-owned: pruning through an internal symlink could remove an
// unrelated file even when its bytes happen to match a previous generated file.
fn unchanged(root: &Path, relative: &Path, expected: &str) -> Result<bool> {
    validate_destination(root, relative)?;
    let path = root.join(relative);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= 64 * 1024 * 1024 => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    if path.canonicalize()? != path {
        return Ok(false);
    }
    Ok(hash(&fs::read(path)?) == expected)
}

pub(super) fn stage_package(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    removals: &mut BTreeMap<PathBuf, String>,
    name: &str,
    target: &str,
    registry_exported: bool,
    source: &str,
) -> Result<()> {
    let directory = package_at(root, name, target)?;
    let previous = read_ownership(root, name, target)?
        .map(|record| record.files)
        .unwrap_or_default();
    let mut owned: BTreeMap<_, _> = files
        .iter()
        .filter(|(path, _)| path.starts_with(&directory))
        .map(|(path, content)| (path.clone(), hash(content.as_bytes())))
        .collect();
    for (relative, expected) in previous {
        if owned.contains_key(&relative) {
            continue;
        }
        // Keep the original hash when users edit compiler options: checking
        // generated code must respect those options, and later cleanup still
        // recognizes that the edited configuration belongs to the user.
        if target == "typescript" && relative == directory.join("tsconfig.json") {
            owned.insert(relative, expected);
        } else if unchanged(root, &relative, &expected)? {
            removals.insert(relative, expected);
        }
    }
    stage_record(
        root,
        files,
        name,
        target,
        registry_exported,
        (source == "asyncapi").then_some(source),
        owned,
    )
}

/// Each generated package's ownership record sits beside the package (so it
/// survives the package's retirement and the owning project's directory);
/// AsyncAPI ownership sits beside that project's socket artifacts. Neither uses
/// the application's `.forklaunch/`.
fn marker_with(
    root: &Path,
    name: &str,
    target: &str,
    metadata: Option<&toml::Table>,
) -> Result<PathBuf> {
    if target == "asyncapi" {
        return Ok(crate::core::artifact_paths::socket_receipt(name, metadata, target));
    }
    let package = package_at(root, name, target)?;
    let file = package
        .file_name()
        .context("Socket SDK package name missing")?
        .to_string_lossy()
        .into_owned();
    Ok(package.with_file_name(format!(".{file}{PACKAGE_RECEIPT_SUFFIX}")))
}

const PACKAGE_RECEIPT_SUFFIX: &str = ".forklaunch.json";

fn marker(root: &Path, name: &str, target: &str) -> Result<PathBuf> {
    if target != "asyncapi" {
        return marker_with(root, name, target, None);
    }
    let metadata = crate::core::artifact_paths::application_metadata(root, name);
    marker_with(root, name, target, metadata.as_ref())
}

pub(super) fn is_receipt(relative: &Path) -> bool {
    relative.file_name().is_some_and(|file| {
        let file = file.to_string_lossy();
        (file.starts_with('.') && file.ends_with(PACKAGE_RECEIPT_SUFFIX))
            || file == "socket-sdk-asyncapi.json"
    }) || relative.starts_with(".forklaunch/socket-sdk-outputs")
}

#[cfg(test)]
pub(super) fn package_receipt(root: &Path, name: &str, target: &str) -> PathBuf {
    marker_with(root, name, target, None).unwrap()
}

fn read_ownership(root: &Path, name: &str, target: &str) -> Result<Option<Ownership>> {
    read_ownership_at(root, name, target, &marker(root, name, target)?)
}

fn read_ownership_at(
    root: &Path,
    name: &str,
    target: &str,
    marker: &Path,
) -> Result<Option<Ownership>> {
    validate_destination(root, marker)?;
    match fs::File::open(root.join(marker)) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                bail!("Socket SDK ownership record exceeds 1 MiB");
            }
            let previous: Ownership =
                serde_json::from_slice(&bytes).context("Invalid socket SDK ownership record")?;
            if previous.format_version != 1
                || previous.files.len() > 4096
                || previous
                    .source
                    .as_deref()
                    .is_some_and(|source| source != "asyncapi")
            {
                bail!("Unsupported socket SDK ownership record");
            }
            let directory = if target != "asyncapi" {
                Some(package_at(root, name, target)?)
            } else {
                None
            };
            if target == "asyncapi" && previous.files.len() > 1 {
                bail!("Invalid socket AsyncAPI ownership record");
            }
            for (relative, expected) in &previous.files {
                if directory
                    .as_ref()
                    .is_some_and(|directory| !relative.starts_with(directory))
                    || expected.len() != 64
                    || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    bail!("Invalid socket SDK ownership entry");
                }
                validate_destination(root, relative)?;
            }
            Ok(Some(previous))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn owned_targets(root: &Path, name: &str) -> Result<Vec<&'static str>> {
    let mut targets = Vec::new();
    for target in ["typescript", "rust"] {
        if read_ownership(root, name, target)?.is_some() {
            targets.push(target);
        }
    }
    Ok(targets)
}

pub(super) fn rename_service(
    root: &Path,
    old: &str,
    new: &str,
    stdout: &mut StandardStream,
) -> Result<()> {
    let targets = owned_targets(root, old)?;
    let mut imported_targets = Vec::new();
    for target in &targets {
        if read_ownership(root, old, target)?
            .is_some_and(|record| record.source.as_deref() == Some("asyncapi"))
        {
            imported_targets.push(*target);
        }
    }
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    let project = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .and_then(|projects| {
            projects
                .iter()
                .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(new))
        })
        .context("Renamed socket service missing from manifest")?;
    let metadata = project.get("metadata").and_then(toml::Value::as_table);
    let contract_path = PathBuf::from(artifact_paths::path(new, metadata, Artifact::Contract));
    let contract = read_json(root, &contract_path, 16 * 1024 * 1024)?;
    let registered = crate::core::openapi_export::rename_registered_sockets(
        root,
        old,
        new,
        metadata,
        contract.as_ref(),
    )?;
    let configured = metadata
        .and_then(|metadata| metadata.get("socketContractArtifactPath"))
        .and_then(toml::Value::as_str);
    let new_default = artifact_paths::default_path(new, metadata, Artifact::SocketContract);
    let new_artifact = configured.map_or_else(|| new_default.clone(), str::to_owned);
    // A service-owned SDK folder moved with its crate; legacy defaults are per name.
    let old_default = artifact_paths::default_path(old, metadata, Artifact::SocketContract);
    let old_artifact = configured
        .filter(|configured| *configured != new_default)
        .map_or_else(|| old_default, str::to_owned);
    if !registered {
        if let Some(mut artifact) = read_json(root, Path::new(&old_artifact), 16 * 1024 * 1024)? {
            if artifact["title"] == old {
                artifact["title"] = json!(new);
            }
            validate_destination(root, Path::new(&new_artifact))?;
            let destination = root.join(&new_artifact);
            fs::create_dir_all(destination.parent().context("Socket artifact parent")?)?;
            fs::write(&destination, pretty(&artifact)?)?;
            if old_artifact != new_artifact {
                fs::remove_file(root.join(old_artifact))?;
            }
        }
    }
    let has_socket_artifact = root.join(&new_artifact).is_file();
    let has_artifact = has_socket_artifact || !imported_targets.is_empty();
    if has_artifact && targets.contains(&"typescript") {
        let source = package_at(root, old, "typescript")?.join("tsconfig.json");
        let destination = package_at(root, new, "typescript")?.join("tsconfig.json");
        validate_destination(root, &source)?;
        validate_destination(root, &destination)?;
        if root.join(&source).is_file() {
            let mut config = Vec::new();
            fs::File::open(root.join(source))?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut config)?;
            if config.len() > 1024 * 1024 {
                bail!("Socket TypeScript configuration exceeds 1 MiB");
            }
            fs::create_dir_all(root.join(destination.parent().unwrap()))?;
            fs::write(root.join(destination), config)?;
        }
    }
    if cleanup_service(root, old, None)? {
        crate::init::rust_project::remove_cargo_member(
            root,
            &package_at(root, old, "rust")?.to_string_lossy(),
        )?;
    }
    for source in ["asyncapi", "socket-contract"] {
        let selected: Vec<_> = targets
            .iter()
            .copied()
            .filter(|target| imported_targets.contains(target) == (source == "asyncapi"))
            .collect();
        if !selected.is_empty() && (source == "asyncapi" || has_socket_artifact) {
            let target = if selected.len() == 2 {
                "all"
            } else {
                selected[0]
            };
            generate_from(root, target, source, Some(new), false, stdout, false)?;
        }
    }
    Ok(())
}

pub(super) fn release_asyncapi(
    root: &Path,
    removals: &mut BTreeMap<PathBuf, String>,
    name: &str,
) -> Result<()> {
    if read_ownership(root, name, "asyncapi")?.is_some() {
        let marker = marker(root, name, "asyncapi")?;
        removals.insert(marker.clone(), hash(&fs::read(root.join(marker))?));
    }
    Ok(())
}

pub(super) fn imported_service(root: &Path, name: &str) -> Result<bool> {
    for target in ["typescript", "rust"] {
        if read_ownership(root, name, target)?
            .is_some_and(|record| record.source.as_deref() == Some("asyncapi"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn imported_target(root: &Path, name: &str, target: &str) -> Result<bool> {
    Ok(read_ownership(root, name, target)?
        .is_some_and(|record| record.source.as_deref() == Some("asyncapi")))
}

pub(super) fn stage_asyncapi(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    removals: &mut BTreeMap<PathBuf, String>,
    name: &str,
    output: &Path,
    registry_exported: bool,
) -> Result<()> {
    let normalized: PathBuf = output
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect();
    let output = normalized.as_path();
    let content = files
        .get(output)
        .context("Staged socket AsyncAPI missing")?;
    let owned = BTreeMap::from([(output.to_path_buf(), hash(content.as_bytes()))]);
    if let Some(previous) = read_ownership(root, name, "asyncapi")? {
        for (relative, expected) in previous.files {
            if relative != output && unchanged(root, &relative, &expected)? {
                removals.insert(relative, expected);
            }
        }
    }
    stage_record(
        root,
        files,
        name,
        "asyncapi",
        registry_exported,
        None,
        owned,
    )
}

pub(super) fn stage_empty_asyncapi(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    removals: &mut BTreeMap<PathBuf, String>,
    name: &str,
) -> Result<()> {
    if let Some(previous) = read_ownership(root, name, "asyncapi")? {
        if previous.registry_exported {
            for (relative, expected) in previous.files {
                if unchanged(root, &relative, &expected)? {
                    removals.insert(relative, expected);
                }
            }
            stage_record(root, files, name, "asyncapi", true, None, BTreeMap::new())?;
        }
    }
    Ok(())
}

/// Explicit project deletion retires its generated outputs even for offline
/// artifacts. Hashes retain application edits, custom compiler files and aliases.
/// `metadata` locates AsyncAPI ownership for a project already removed from the manifest.
pub(super) fn cleanup_service(
    root: &Path,
    name: &str,
    metadata: Option<&toml::Table>,
) -> Result<bool> {
    let mut removals = BTreeMap::new();
    let mut rust_owned = false;
    for target in ["typescript", "rust", "asyncapi"] {
        let marker = match metadata {
            Some(metadata) => marker_with(root, name, target, Some(metadata))?,
            None => marker(root, name, target)?,
        };
        let Some(previous) = read_ownership_at(root, name, target, &marker)? else {
            continue;
        };
        rust_owned |= target == "rust";
        removals.extend(previous.files);
        removals.insert(marker.clone(), hash(&fs::read(root.join(marker))?));
    }
    remove_unchanged(root, &removals)?;
    Ok(rust_owned)
}

fn stage_record(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    name: &str,
    target: &str,
    registry_exported: bool,
    source: Option<&str>,
    owned: BTreeMap<PathBuf, String>,
) -> Result<()> {
    let marker = marker(root, name, target)?;
    validate_destination(root, &marker)?;
    stage(
        files,
        marker,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Ownership {
                format_version: 1,
                registry_exported,
                source: source.map(str::to_owned),
                files: owned,
            })?
        ),
    )
}

fn read_json(root: &Path, relative: &Path, limit: u64) -> Result<Option<Value>> {
    validate_destination(root, relative)?;
    let file = match fs::File::open(root.join(relative)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("Socket ownership input exceeds its size limit");
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub(super) fn registry_exported(
    root: &Path,
    name: &str,
    metadata: Option<&toml::Table>,
    artifact: &Path,
) -> Result<bool> {
    let Some(marker) = read_json(
        root,
        &artifact_paths::socket_registry_marker(name, metadata),
        4096,
    )?
    else {
        return Ok(false);
    };
    let output = marker["outputPath"]
        .as_str()
        .context("Invalid registry export path")?;
    let expected = marker["sha256"]
        .as_str()
        .context("Invalid registry export hash")?;
    validate_destination(root, Path::new(output))?;
    Ok(marker["formatVersion"] == 1
        && root.join(output).canonicalize()? == artifact
        && unchanged(root, Path::new(output), expected)?)
}

pub(super) struct RegistrySnapshot {
    name: String,
    metadata: Option<toml::Table>,
    artifact: PathBuf,
    contract: PathBuf,
    value: Value,
}

impl RegistrySnapshot {
    pub(super) fn verify(&self, root: &Path) -> Result<()> {
        anyhow::ensure!(
            read_json(root, &self.contract, 16 * 1024 * 1024)?.as_ref() == Some(&self.value)
                && registry_exported(root, &self.name, self.metadata.as_ref(), &self.artifact)?,
            "Socket registry changed during SDK compilation; retry after export completes"
        );
        Ok(())
    }
}

/// Build scripts emit the HTTP registry before runtime export updates its socket
/// projection. Never regenerate from the previous projection during that gap:
/// doing so could overwrite edits immediately before retiring those same files.
pub(super) fn registry_snapshot(
    root: &Path,
    name: &str,
    artifact: &Path,
    project: &toml::Value,
) -> Result<Option<RegistrySnapshot>> {
    let metadata = project.get("metadata").and_then(toml::Value::as_table);
    if !registry_exported(root, name, metadata, artifact)? {
        // Independently authored/edited portable artifacts remain valid inputs.
        return Ok(None);
    }
    let contract = PathBuf::from(artifact_paths::path(name, metadata, Artifact::Contract));
    let value = read_json(root, &contract, 16 * 1024 * 1024)?
        .context("Socket registry is unavailable; retry after export completes")?;
    let portable = read_json(root, artifact.strip_prefix(root)?, 16 * 1024 * 1024)?
        .context("Socket projection is unavailable; retry after export completes")?;
    anyhow::ensure!(
        value["generator"] == "framework-rs"
            && portable["formatVersion"] == 1
            && portable["title"] == value["service"]["name"]
            && portable["version"] == value["service"]["version"]
            && portable["sockets"].is_object()
            && portable["sockets"] == value["sockets"],
        "Socket projection does not match the current service registry; retry after export completes"
    );
    Ok(Some(RegistrySnapshot {
        name: name.into(),
        metadata: metadata.cloned(),
        artifact: artifact.to_path_buf(),
        contract,
        value,
    }))
}

/// A missing artifact alone may be a transient editor operation. Only prune
/// after a previously owned registry export and a fresh, explicitly empty
/// framework contract agree that the last registered socket was removed.
pub(super) fn stage_empty_registry(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    removals: &mut BTreeMap<PathBuf, String>,
    name: &str,
    target: &str,
    project: &toml::Value,
) -> Result<bool> {
    let Some(previous) = read_ownership(root, name, target)? else {
        return Ok(false);
    };
    if !previous.registry_exported {
        return Ok(false);
    }
    let contract = artifact_paths::path(
        name,
        project.get("metadata").and_then(toml::Value::as_table),
        Artifact::Contract,
    );
    let Some(contract) = read_json(root, Path::new(&contract), 16 * 1024 * 1024)? else {
        return Ok(false);
    };
    if contract["generator"] != "framework-rs"
        || contract["service"]["name"] != name
        || contract.get("sockets").is_some_and(|sockets| {
            !sockets
                .as_object()
                .is_some_and(|sockets| sockets.is_empty())
        })
    {
        return Ok(false);
    }
    for (relative, expected) in previous.files {
        if unchanged(root, &relative, &expected)? {
            removals.insert(relative, expected);
        }
    }
    stage_record(root, files, name, target, true, None, BTreeMap::new())?;
    Ok(true)
}

pub(super) fn remove_unchanged(root: &Path, removals: &BTreeMap<PathBuf, String>) -> Result<()> {
    for (relative, expected) in removals {
        if unchanged(root, relative, expected)? {
            fs::remove_file(root.join(relative))?;
        }
    }
    let base = crate::sdk::sdk_base(root)?;
    let mut directories = std::collections::BTreeSet::new();
    for relative in removals.keys() {
        let mut parent = relative.parent();
        while let Some(directory) = parent {
            if directory.components().count() < base.components().count() + 2
                || !(directory.starts_with(base.join("rust"))
                    || directory.starts_with(base.join("typescript-websocket")))
            {
                break;
            }
            directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    for relative in directories.iter().rev() {
        let directory = root.join(relative);
        if directory.canonicalize().ok().as_ref() != Some(&directory) {
            continue;
        }
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rename_reads_local_custom_and_legacy_contract_paths() {
        for configured in [
            Some("src/new/sdk/typescript/contract.json"),
            Some("custom/contracts/new.json"),
            None,
        ] {
            let fixture = tempfile::tempdir().unwrap();
            let root = fixture.path().canonicalize().unwrap();
            fs::create_dir_all(root.join(".forklaunch")).unwrap();
            let metadata = configured.map_or_else(String::new, |path| {
                format!("[projects.metadata]\ncontractArtifactPath = {path:?}\n")
            });
            fs::write(
                root.join(".forklaunch/manifest.toml"),
                format!("modules_path = 'src'\n[[projects]]\nname = 'new'\n{metadata}"),
            )
            .unwrap();
            let mut artifact = json!({
                "generator": "framework-rs",
                "service": {"name": "old", "version": "1.0.0"},
                "sockets": {"chat": {"path": "/chat"}},
            });
            crate::core::openapi_export::export_registered_sockets(
                &root, "old", None, &artifact,
            )
            .unwrap();
            artifact["service"]["name"] = json!("new");
            let contract = root.join(
                configured.unwrap_or(".forklaunch/contracts/new/contract.json"),
            );
            fs::create_dir_all(contract.parent().unwrap()).unwrap();
            let content = serde_json::to_vec(&artifact).unwrap();
            fs::write(&contract, &content).unwrap();
            rename_service(
                &root,
                "old",
                "new",
                &mut StandardStream::stdout(termcolor::ColorChoice::Never),
            )
            .unwrap();
            let portable: Value = serde_json::from_slice(
                &fs::read(root.join(".forklaunch/socket-contracts/new/socket-contract.json"))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(portable["title"], "new");
            assert_eq!(portable["sockets"], artifact["sockets"]);
            assert!(!root.join(".forklaunch/socket-contracts/old").exists());
            assert_eq!(fs::read(contract).unwrap(), content);
        }
    }

    #[test]
    fn registry_rename_rejects_missing_or_escaping_explicit_contract() {
        for configured in ["src/new/sdk/typescript/missing.json", "../outside.json"] {
            let fixture = tempfile::tempdir().unwrap();
            let root = fixture.path().canonicalize().unwrap();
            fs::create_dir_all(root.join(".forklaunch/contracts/new")).unwrap();
            fs::write(
                root.join(".forklaunch/manifest.toml"),
                format!("modules_path = 'src'\n[[projects]]\nname = 'new'\n[projects.metadata]\ncontractArtifactPath = {configured:?}\n"),
            )
            .unwrap();
            let artifact = json!({
                "generator": "framework-rs",
                "service": {"name": "old", "version": "1.0.0"},
                "sockets": {"chat": {"path": "/chat"}},
            });
            crate::core::openapi_export::export_registered_sockets(
                &root, "old", None, &artifact,
            )
            .unwrap();
            fs::write(
                root.join(".forklaunch/contracts/new/contract.json"),
                serde_json::to_vec(&artifact).unwrap(),
            )
            .unwrap();
            let marker = root.join(".forklaunch/socket-contracts/old/registry-export.json");
            let before = fs::read(&marker).unwrap();
            assert!(rename_service(
                &root,
                "old",
                "new",
                &mut StandardStream::stdout(termcolor::ColorChoice::Never),
            )
            .is_err());
            assert_eq!(fs::read(marker).unwrap(), before);
            assert!(!root.join(".forklaunch/socket-contracts/new").exists());
        }
    }

    fn write_staged(root: &Path, files: &BTreeMap<PathBuf, String>) {
        for (relative, content) in files {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    #[test]
    fn empty_registry_cleanup_requires_positive_provenance_and_fresh_contract() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let source = package("chat", "rust").join("src/socket_chat.rs");
        let mut files = BTreeMap::from([(source.clone(), "original".into())]);
        stage_package(
            &root,
            &mut files,
            &mut BTreeMap::new(),
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        write_staged(&root, &files);
        let contract = root.join(".forklaunch/contracts/chat/contract.json");
        fs::create_dir_all(contract.parent().unwrap()).unwrap();
        fs::write(
            &contract,
            r#"{"generator":"framework-rs","service":{"name":"chat"}}"#,
        )
        .unwrap();
        let project = toml::Value::Table(Default::default());
        let mut staged = BTreeMap::new();
        let mut removals = BTreeMap::new();
        assert!(
            !stage_empty_registry(&root, &mut staged, &mut removals, "chat", "rust", &project)
                .unwrap()
        );
        files.retain(|path, _| path == &source);
        stage_package(
            &root,
            &mut files,
            &mut BTreeMap::new(),
            "chat",
            "rust",
            true,
            "socket-contract",
        )
        .unwrap();
        write_staged(&root, &files);
        fs::write(
            &contract,
            r#"{"generator":"framework-rs","service":{"name":"chat"},"sockets":{"chat":{}}}"#,
        )
        .unwrap();
        assert!(
            !stage_empty_registry(&root, &mut staged, &mut removals, "chat", "rust", &project)
                .unwrap()
        );
        fs::write(
            &contract,
            r#"{"generator":"framework-rs","service":{"name":"chat"},"sockets":{}}"#,
        )
        .unwrap();
        assert!(
            stage_empty_registry(&root, &mut staged, &mut removals, "chat", "rust", &project)
                .unwrap()
        );
        assert!(root.join(&source).exists());
        remove_unchanged(&root, &removals).unwrap();
        write_staged(&root, &staged);
        assert!(!root.join(package("chat", "rust")).exists());
        let mut repeated = BTreeMap::new();
        assert!(
            stage_empty_registry(
                &root,
                &mut repeated,
                &mut BTreeMap::new(),
                "chat",
                "rust",
                &project
            )
            .unwrap()
        );
        assert_eq!(repeated, staged);
    }

    #[test]
    fn explicit_service_cleanup_preserves_edits_and_prunes_owned_asyncapi() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let source = package("chat", "rust").join("src/socket_chat.rs");
        let edited = package("chat", "rust").join("Cargo.toml");
        let manual = package("chat", "rust").join("src/user.rs");
        let mut files = BTreeMap::from([
            (source.clone(), "original".into()),
            (edited.clone(), "generated".into()),
        ]);
        stage_package(
            &root,
            &mut files,
            &mut BTreeMap::new(),
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        let api = PathBuf::from("custom/asyncapi.json");
        files.insert(api.clone(), "{}".into());
        stage_asyncapi(
            &root,
            &mut files,
            &mut BTreeMap::new(),
            "chat",
            Path::new("./custom/asyncapi.json"),
            false,
        )
        .unwrap();
        write_staged(&root, &files);
        fs::write(root.join(&edited), "user edits").unwrap();
        fs::write(root.join(&manual), "user file").unwrap();
        assert!(cleanup_service(&root, "chat", None).unwrap());
        assert!(!root.join(source).exists());
        assert!(!root.join(api).exists());
        assert!(!root.join(marker(&root, "chat", "rust").unwrap()).exists());
        assert_eq!(fs::read_to_string(root.join(edited)).unwrap(), "user edits");
        assert_eq!(fs::read_to_string(root.join(manual)).unwrap(), "user file");
        assert!(!cleanup_service(&root, "chat", None).unwrap());
    }

    #[test]
    fn prunes_only_unchanged_obsolete_modules_and_keeps_owned_config() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let base = package("chat", "typescript");
        let obsolete = base.join("src/socket_old.ts");
        let edited = base.join("src/socket_edited.ts");
        let config = base.join("tsconfig.json");
        let mut first = BTreeMap::from([
            (obsolete.clone(), "old".into()),
            (edited.clone(), "original".into()),
            (config.clone(), "{}".into()),
        ]);
        stage_package(
            &root,
            &mut first,
            &mut BTreeMap::new(),
            "chat",
            "typescript",
            false,
            "socket-contract",
        )
        .unwrap();
        write_staged(&root, &first);
        fs::write(root.join(&edited), "user edits").unwrap();
        let mut next = BTreeMap::from([(base.join("src/socket_new.ts"), "new".into())]);
        let mut removals = BTreeMap::new();
        stage_package(
            &root,
            &mut next,
            &mut removals,
            "chat",
            "typescript",
            false,
            "socket-contract",
        )
        .unwrap();
        assert_eq!(removals.keys().collect::<Vec<_>>(), vec![&obsolete]);
        assert!(root.join(&obsolete).exists(), "staging must be read-only");
        remove_unchanged(&root, &removals).unwrap();
        write_staged(&root, &next);
        assert!(!root.join(obsolete).exists());
        assert_eq!(fs::read_to_string(root.join(edited)).unwrap(), "user edits");
        assert_eq!(fs::read_to_string(root.join(config)).unwrap(), "{}");
        let mut repeat = BTreeMap::from([(base.join("src/socket_new.ts"), "new".into())]);
        stage_package(
            &root,
            &mut repeat,
            &mut BTreeMap::new(),
            "chat",
            "typescript",
            false,
            "socket-contract",
        )
        .unwrap();
        assert_eq!(next, repeat, "regeneration should be stable");
    }

    #[test]
    fn rechecks_ownership_before_removal_and_rejects_escape_entries() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let relative = package("chat", "rust").join("src/socket_old.rs");
        let mut first = BTreeMap::from([(relative.clone(), "original".into())]);
        stage_package(
            &root,
            &mut first,
            &mut BTreeMap::new(),
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        write_staged(&root, &first);
        let mut removals = BTreeMap::new();
        stage_package(
            &root,
            &mut BTreeMap::new(),
            &mut removals,
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        fs::write(root.join(&relative), "concurrent edits").unwrap();
        remove_unchanged(&root, &removals).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(relative)).unwrap(),
            "concurrent edits"
        );
        let marker = root.join(package_receipt(&root, "chat", "rust"));
        fs::write(marker, serde_json::to_vec(&json!({"formatVersion":1,"files":{"sdks/rust/chat-websocket-sdk/../../../escape":hash(b"x")}})).unwrap()).unwrap();
        assert!(
            stage_package(
                &root,
                &mut BTreeMap::new(),
                &mut removals,
                "chat",
                "rust",
                false,
                "socket-contract"
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn does_not_prune_internal_symlink_aliases() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let relative = package("chat", "rust").join("src/socket_old.rs");
        let mut files = BTreeMap::from([(relative.clone(), "original".into())]);
        stage_package(
            &root,
            &mut files,
            &mut BTreeMap::new(),
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        write_staged(&root, &files);
        fs::rename(root.join(&relative), root.join("user.rs")).unwrap();
        std::os::unix::fs::symlink(root.join("user.rs"), root.join(&relative)).unwrap();
        let mut removals = BTreeMap::new();
        stage_package(
            &root,
            &mut BTreeMap::new(),
            &mut removals,
            "chat",
            "rust",
            false,
            "socket-contract",
        )
        .unwrap();
        assert!(removals.is_empty());
        assert!(root.join(relative).exists());
    }
}
