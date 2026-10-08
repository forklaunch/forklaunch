//! Read-only preparation from a separately built snapshot. Never executes project code.
use std::{
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};

use crate::core::manifest::{ProjectType, application::ApplicationManifestData};

pub(super) fn verify_application(id: &str, token: &str) -> Result<()> {
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .get(format!(
            "{}/applications/{id}",
            crate::constants::get_platform_management_api_url()
        ))
        .bearer_auth(token)
        .send()
        .context("Cannot verify the selected application")?;
    if !response.status().is_success() {
        bail!(
            "Cannot access the selected application (HTTP {})",
            response.status()
        );
    }
    let app: serde_json::Value = response.json().context("Invalid application response")?;
    if app["id"].as_str() != Some(id) {
        bail!("The server returned a different application");
    }
    Ok(())
}
fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("Prebuilt paths must stay inside the application snapshot");
    }
    Ok(())
}
pub(super) fn spec_directory(root: &Path, path: &str) -> Result<PathBuf> {
    relative(Path::new(path))?;
    let resolved = root
        .join(path)
        .canonicalize()
        .context("Prebuilt API directory is missing")?;
    if !resolved.starts_with(root.canonicalize()?) {
        bail!("Prebuilt API directory leaves the application snapshot");
    }
    Ok(resolved)
}
pub(super) fn validate_paths(root: &Path, manifest: &ApplicationManifestData) -> Result<()> {
    relative(Path::new(&manifest.modules_path))?;
    spec_directory(root, &manifest.modules_path)?;
    for project in &manifest.projects {
        if project.name.is_empty()
            || !project
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("Invalid project name in prebuilt snapshot");
        }
        spec_directory(root, &format!("{}/{}", manifest.modules_path, project.name))?;
    }
    Ok(())
}
pub(super) fn validate_specs(
    dir: &Path,
    manifest: &ApplicationManifestData,
) -> Result<Vec<String>> {
    let mut services = Vec::new();
    for project in &manifest.projects {
        if project.r#type != ProjectType::Service {
            continue;
        }
        let path = dir.join(&project.name).join("openapi.json");
        let canonical = path
            .canonicalize()
            .context("A service has no prebuilt API specification")?;
        if !canonical.starts_with(dir) || std::fs::metadata(&canonical)?.len() > 20 * 1024 * 1024 {
            bail!("Invalid prebuilt API specification path or size");
        }
        let spec: serde_json::Value = serde_json::from_slice(&std::fs::read(canonical)?)?;
        if !spec["openapi"]
            .as_str()
            .is_some_and(|v| v.starts_with("3."))
            || !spec["paths"].is_object()
        {
            bail!(
                "Invalid prebuilt OpenAPI specification for {}",
                project.name
            );
        }
        services.push(project.name.clone());
    }
    Ok(services)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_external_or_missing_spec_directory() {
        let root = tempfile::tempdir().unwrap();
        for path in ["", "../outside", "/tmp", "missing"] {
            assert!(spec_directory(root.path(), path).is_err());
        }
        std::fs::create_dir(root.path().join("api")).unwrap();
        assert!(spec_directory(root.path(), "api").is_ok());
    }
    #[test]
    #[cfg(unix)]
    fn rejects_directory_link_outside_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("api")).unwrap();
        assert!(spec_directory(root.path(), "api").is_err());
    }
}
