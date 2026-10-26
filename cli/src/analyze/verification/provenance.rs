//! Build-owned HTTP evidence, limited to its explicitly declared source scope.
use super::Check;
use crate::core::{manifest::ProjectEntry, static_analysis::workspace_diagnostics};
use std::{
    fs,
    path::{Component, Path},
};

pub(super) fn inspect(root: &Path, modules: &str, project: &ProjectEntry) -> Check {
    let mut check = Check {
        kind: "http_provenance_declared_scope",
        project: project.name.clone(),
        command: vec![],
        status: "not_checked",
        exit_code: None,
        message: None,
    };
    let result = (|| -> anyhow::Result<bool> {
        let artifact = project
            .metadata
            .as_ref()
            .and_then(|m| m.contract_artifact_path.as_deref())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(modules)
                    .join("client-sdk/typescript")
                    .join(&project.name)
                    .join("contract.json")
            });
        anyhow::ensure!(
            !artifact.is_absolute()
                && !artifact.components().any(|c| matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )),
            "Contract artifact must be application-relative"
        );
        let receipt = root.join(artifact).with_file_name("provenance.json");
        if !receipt.exists() {
            return Ok(false);
        }
        anyhow::ensure!(
            receipt.canonicalize()?.starts_with(root.canonicalize()?),
            "Provenance path leaves application"
        );
        anyhow::ensure!(
            fs::metadata(&receipt)?.len() <= 4 * 1024 * 1024,
            "Provenance manifest exceeds size limit"
        );
        let manifest: forklaunch_core::build::provenance::Manifest =
            serde_json::from_slice(&fs::read(receipt)?)?;
        anyhow::ensure!(
            manifest.service == project.name,
            "Provenance service does not match selected project"
        );
        let directory = workspace_diagnostics::project_dir(root, modules, project)
            .ok_or_else(|| anyhow::anyhow!("Invalid service directory"))?;
        manifest.verify_service_scope(&directory, &manifest.context)?;
        Ok(true)
    })();
    match result {
        Ok(true) => {
            check.status = "passed";
            check.message = Some("Declared source/configuration scope, HTTP contract, both SDKs and explicitly recorded additional asset digests match the recorded build context. Independent target/features, complete dependency graph and runtime binding/security behavior are not established.".into());
        }
        Ok(false) => check.message = Some("No build-owned HTTP provenance manifest; rebuild with a provenance-enabled framework hook".into()),
        Err(error) => {
            check.status = "failed";
            check.message = Some(format!("{error:#}"));
        }
    }
    check
}

/// Inspect the conventional worker channel receipt when that asset surface exists.
/// This does not infer ownership from arbitrary files or execute build extensions.
pub(super) fn inspect_channels(
    root: &Path,
    modules: &str,
    projects: &[ProjectEntry],
    filter: Option<&str>,
) -> Option<Check> {
    inspect_channel_at(
        root,
        modules,
        projects,
        filter,
        &root.join(modules).join("client-sdk/events"),
    )
}

pub(super) fn inspect_local_channels(
    root: &Path,
    modules: &str,
    projects: &[ProjectEntry],
    filter: Option<&str>,
) -> Vec<Check> {
    projects
        .iter()
        .filter(|project| filter.is_none_or(|name| name == project.name))
        .filter_map(|project| {
            let directory =
                workspace_diagnostics::project_dir(root, modules, project)?.join("events");
            // Producer worker channels are already included in the HTTP receipt. Standalone
            // workers require their own receipt; never silently skip missing evidence.
            if !directory.join("provenance.json").exists()
                && project.metadata.as_ref().and_then(|m| m.contract_export) != Some(false)
            {
                return None;
            }
            inspect_channel_at(root, modules, projects, Some(&project.name), &directory)
        })
        .collect()
}

fn inspect_channel_at(
    root: &Path,
    modules: &str,
    projects: &[ProjectEntry],
    filter: Option<&str>,
    directory: &Path,
) -> Option<Check> {
    if !directory.join("event-contract.json").exists()
        && !directory.join("provenance.json").exists()
    {
        return None;
    }
    let mut check = Check {
        kind: "channel_provenance_declared_scope",
        project: filter.unwrap_or("workspace").into(),
        command: vec![],
        status: "not_checked",
        exit_code: None,
        message: None,
    };
    let result = (|| -> anyhow::Result<bool> {
        let receipt = directory.join("provenance.json");
        if !receipt.exists() {
            return Ok(false);
        }
        anyhow::ensure!(
            receipt.canonicalize()?.starts_with(root.canonicalize()?),
            "Channel provenance path leaves application"
        );
        anyhow::ensure!(
            fs::metadata(&receipt)?.len() <= 4 * 1024 * 1024,
            "Channel provenance manifest exceeds size limit"
        );
        let manifest: forklaunch_core::build::provenance::AssetManifest =
            serde_json::from_slice(&fs::read(receipt)?)?;
        if filter.is_some_and(|name| name != manifest.service) {
            return Ok(false);
        }
        let project = projects
            .iter()
            .find(|project| project.name == manifest.service)
            .ok_or_else(|| {
                anyhow::anyhow!("Channel provenance owner is not a registered project")
            })?;
        anyhow::ensure!(
            project
                .metadata
                .as_ref()
                .and_then(|m| m.language.as_deref())
                == Some("rust"),
            "Channel provenance owner must be Rust"
        );
        check.project = project.name.clone();
        let manifest_dir = workspace_diagnostics::project_dir(root, modules, project)
            .ok_or_else(|| anyhow::anyhow!("Invalid channel owner directory"))?;
        manifest.verify_service_scope(&manifest_dir, &manifest.context)?;
        Ok(true)
    })();
    match result {
        Ok(true) => {
            check.status = "passed";
            check.message = Some("Declared channel input and output digests match the recorded build context; runtime encryption, authorization and independent target/features are not attested.".into());
        }
        Ok(false) => {
            check.message = Some("No channel provenance receipt for the selected owner; channel freshness is not established".into());
        }
        Err(error) => {
            check.status = "failed";
            check.message = Some(format!("{error:#}"));
        }
    }
    Some(check)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(artifact: &str) -> ProjectEntry {
        serde_json::from_value(serde_json::json!({
            "name":"notes", "description":"", "type":"Service",
            "metadata":{"language":"rust", "contractArtifactPath":artifact}
        }))
        .unwrap()
    }
    #[test]
    fn project_local_channel_inventory_checks_standalone_and_respects_filter() {
        let root = tempfile::tempdir().unwrap();
        let mut worker = project("src/notes/sdk/typescript/contract.json");
        worker.metadata.as_mut().unwrap().contract_export = Some(false);
        let directory = root.path().join("src/notes/events");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("event-contract.json"), "{}").unwrap();
        let checks = inspect_local_channels(root.path(), "src", &[worker.clone()], None);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, "not_checked");
        fs::write(directory.join("provenance.json"), "{}").unwrap();
        assert_eq!(
            inspect_local_channels(root.path(), "src", &[worker.clone()], None)[0].status,
            "failed"
        );
        assert!(inspect_local_channels(root.path(), "src", &[worker], Some("other")).is_empty());
    }

    #[test]
    fn absent_evidence_is_unknown_but_malformed_evidence_fails() {
        let root = tempfile::tempdir().unwrap();
        let project = project("src/client-sdk/typescript/notes/contract.json");
        assert_eq!(inspect(root.path(), "src", &project).status, "not_checked");
        let directory = root.path().join("src/client-sdk/typescript/notes");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("provenance.json"), "{}").unwrap();
        assert_eq!(inspect(root.path(), "src", &project).status, "failed");
    }
    #[test]
    fn channel_evidence_is_optional_until_assets_exist_and_malformed_fails() {
        let root = tempfile::tempdir().unwrap();
        assert!(inspect_channels(root.path(), "src", &[], None).is_none());
        let directory = root.path().join("src/client-sdk/events");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("event-contract.json"), "{}").unwrap();
        assert_eq!(
            inspect_channels(root.path(), "src", &[], None)
                .unwrap()
                .status,
            "not_checked"
        );
        fs::write(directory.join("provenance.json"), "{}").unwrap();
        assert_eq!(
            inspect_channels(root.path(), "src", &[], None)
                .unwrap()
                .status,
            "failed"
        );
    }
    #[test]
    #[cfg(unix)]
    fn channel_evidence_cannot_follow_an_external_receipt() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("provenance.json"), "{}").unwrap();
        let directory = root.path().join("src/client-sdk");
        fs::create_dir_all(&directory).unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.join("events")).unwrap();
        let check = inspect_channels(root.path(), "src", &[], None).unwrap();
        assert_eq!(check.status, "failed");
        assert!(check.message.unwrap().contains("leaves application"));
    }
    #[test]
    fn evidence_path_cannot_escape_application() {
        let root = tempfile::tempdir().unwrap();
        for path in ["../outside/contract.json", "/tmp/outside/contract.json"] {
            assert_eq!(inspect(root.path(), "src", &project(path)).status, "failed");
        }
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            fs::write(outside.path().join("provenance.json"), "{}").unwrap();
            std::os::unix::fs::symlink(outside.path(), root.path().join("outside")).unwrap();
            let result = inspect(root.path(), "src", &project("outside/contract.json"));
            assert_eq!(result.status, "failed");
            assert!(result.message.unwrap().contains("leaves application"));
        }
    }
}

pub(super) fn inspect_bindings(root: &Path, modules: &str, project: &ProjectEntry) -> Check {
    let mut check = Check {
        kind: "managed_http_syntactic_assembly",
        project: project.name.clone(),
        command: vec![],
        status: "not_checked",
        exit_code: None,
        message: None,
    };
    let result = (|| -> anyhow::Result<Option<String>> {
        let artifact = project
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.contract_artifact_path.as_deref())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(modules)
                    .join("client-sdk/typescript")
                    .join(&project.name)
                    .join("contract.json")
            });
        anyhow::ensure!(
            !artifact.is_absolute()
                && !artifact.components().any(|part| matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )),
            "Contract path must stay inside workspace"
        );
        let receipt = root.join(&artifact).with_file_name("provenance.json");
        if !receipt.exists() {
            return Ok(None);
        }
        anyhow::ensure!(
            fs::metadata(&receipt)?.len() <= 4 * 1024 * 1024,
            "Provenance exceeds size bound"
        );
        let manifest: forklaunch_core::build::provenance::Manifest =
            serde_json::from_slice(&fs::read(&receipt)?)?;
        let directory = workspace_diagnostics::project_dir(root, modules, project)
            .ok_or_else(|| anyhow::anyhow!("Invalid service directory"))?;
        anyhow::ensure!(
            manifest.service == project.name,
            "Provenance owner mismatch"
        );
        manifest.verify_service_scope(&directory, &manifest.context)?;
        let binding_path = artifact.with_file_name("bindings.json");
        let binding_name = binding_path.to_string_lossy();
        if !manifest.outputs.contains_key(binding_name.as_ref()) {
            return Ok(None);
        }
        let path = root.join(&binding_path);
        anyhow::ensure!(
            path.canonicalize()?.starts_with(root.canonicalize()?),
            "Binding manifest leaves workspace"
        );
        anyhow::ensure!(
            fs::metadata(&path)?.len() <= 4 * 1024 * 1024,
            "Binding manifest exceeds bound"
        );
        let bindings: forklaunch_core::build::provenance::BindingManifest =
            serde_json::from_slice(&fs::read(path)?)?;
        bindings.verify_contract(&serde_json::from_slice(&fs::read(root.join(artifact))?)?)?;
        Ok(Some(super::assembly::verify(
            root,
            &directory,
            &project.name,
            &bindings,
        )?))
    })();
    match result {
        Ok(Some(message)) => {
            check.status = "passed";
            check.message = Some(message)
        }
        Ok(None) => {
            check.message = Some(
                "No digest-bound compiler binding manifest; managed assembly coverage is unknown"
                    .into(),
            )
        }
        Err(error) => {
            if error
                .downcast_ref::<super::assembly::UnmountedOperation>()
                .is_some()
            {
                check.status = "failed";
            }
            check.message = Some(format!("{error:#}"));
        }
    }
    check
}
