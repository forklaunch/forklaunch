//! A typed view of legacy string metadata; never rewrites the user's manifest.
use super::{ProjectLanguage, ProjectType, application::ApplicationManifestData};
use anyhow::{Context, Result, bail, ensure};
use std::path::Path;

impl ProjectLanguage {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
        }
    }
}

pub(crate) fn resolve(
    declared: Option<&str>,
    application: Option<&str>,
    directory: &Path,
) -> Result<ProjectLanguage> {
    ensure!(
        application.is_none_or(|v| matches!(v, "rust" | "typescript" | "mixed")),
        "Unsupported application language"
    );
    ensure!(
        declared.is_none_or(|v| matches!(v, "rust" | "typescript")),
        "Unsupported project language"
    );
    if let (Some(project), Some(app)) = (declared, application) {
        ensure!(
            app == "mixed" || app == project,
            "Project language contradicts application language; declare a mixed application"
        );
    }
    let inferred = match (
        directory.join("Cargo.toml").is_file(),
        directory.join("package.json").is_file(),
    ) {
        (true, false) => Some("rust"),
        (false, true) => Some("typescript"),
        (true, true) if declared.is_none() && application.is_none_or(|v| v == "mixed") => {
            bail!("Ambiguous project language; declare metadata.language")
        }
        _ => None,
    };
    match declared
        .or(application.filter(|v| *v != "mixed"))
        .or(inferred)
    {
        Some("rust") => Ok(ProjectLanguage::Rust),
        Some("typescript") => Ok(ProjectLanguage::TypeScript),
        None if application != Some("mixed") => Ok(ProjectLanguage::TypeScript),
        _ => bail!("Mixed project language cannot be resolved; declare metadata.language"),
    }
}

pub(crate) fn normalized(
    root: &Path,
    manifest: &ApplicationManifestData,
) -> Result<ApplicationManifestData> {
    if let (Some(language), Some(application)) =
        (&manifest.language, &manifest.application_language)
    {
        ensure!(
            language == application,
            "Conflicting language and applicationLanguage metadata"
        );
    }
    let application = manifest
        .language
        .as_deref()
        .or(manifest.application_language.as_deref())
        .or((manifest.runtime == "tokio").then_some("rust"));
    ensure!(
        application.is_none_or(|v| matches!(v, "rust" | "typescript" | "mixed")),
        "Unsupported application language"
    );
    let mut result = manifest.clone();
    for project in &mut result.projects {
        let metadata = project.metadata.get_or_insert_with(Default::default);
        let directory = metadata.crate_path.as_deref().map_or_else(
            || root.join(&manifest.modules_path).join(&project.name),
            |path| root.join(path),
        );
        let language = resolve(metadata.language.as_deref(), application, &directory)
            .with_context(|| format!("Invalid language metadata for {}", project.name))?;
        ensure!(
            metadata.worker_control_service != Some(true)
                || (project.r#type == ProjectType::Worker && language == ProjectLanguage::Rust),
            "workerControlService requires a Rust worker"
        );
        if metadata.worker_service == Some(true) {
            ensure!(project.r#type == ProjectType::Worker && language == ProjectLanguage::Rust,
                "workerService requires a Rust worker");
            ensure!(metadata.contract_export != Some(false), "workerService requires its producer contract export");
            ensure!(metadata.worker_control_service != Some(true), "workerService and legacy workerControlService roles cannot be combined");
        }
        metadata.language = Some(language.as_str().into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_mixed_languages_are_typed_without_silent_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve(None, None, root.path()).unwrap(),
            ProjectLanguage::TypeScript
        );
        assert_eq!(
            resolve(None, Some("rust"), root.path()).unwrap(),
            ProjectLanguage::Rust
        );
        assert!(resolve(Some("go"), Some("mixed"), root.path()).is_err());
        assert!(resolve(Some("rust"), Some("typescript"), root.path()).is_err());
        assert!(resolve(None, Some("mixed"), root.path()).is_err());
        std::fs::write(root.path().join("Cargo.toml"), "").unwrap();
        assert_eq!(
            resolve(None, Some("mixed"), root.path()).unwrap(),
            ProjectLanguage::Rust
        );
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        assert!(resolve(None, Some("mixed"), root.path()).is_err());
        assert_eq!(
            resolve(Some("rust"), Some("mixed"), root.path()).unwrap(),
            ProjectLanguage::Rust
        );
    }
}
