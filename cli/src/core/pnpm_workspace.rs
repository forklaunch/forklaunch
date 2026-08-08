use std::{collections::BTreeMap, fs::read_to_string, path::Path};

use anyhow::{Context, Ok, Result};
use serde::{Deserialize, Serialize};
use serde_yml::{Value, from_str, to_string};

use super::{manifest::InitializableManifestConfig, rendered_template::RenderedTemplate};
use crate::{
    constants::{
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE,
        ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE, ERROR_FAILED_TO_PARSE_PNPM_WORKSPACE,
        ERROR_FAILED_TO_READ_PNPM_WORKSPACE,
    },
    core::manifest::{ManifestConfig, ProjectEntry, ProjectManifestConfig},
};

/// Dependencies the scaffold ships (directly or transitively) that run
/// install-time build scripts. pnpm 10+ blocks build scripts unless they are
/// listed under `allowBuilds`; when the list is missing, pnpm writes literal
/// "set this to true or false" placeholders into pnpm-workspace.yaml, which
/// then fail every subsequent install. Pre-seed real values so pnpm never
/// injects placeholders.
const ALLOWED_BUILD_DEPENDENCIES: &[&str] = &[
    "@scarf/scarf",
    "better-sqlite3",
    "cpu-features",
    "esbuild",
    "msgpackr-extract",
    "protobufjs",
    "sqlite3",
    "ssh2",
    "tldjs",
    "unrs-resolver",
];

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct PnpmWorkspace {
    pub(crate) packages: Vec<String>,
    #[serde(
        rename = "allowBuilds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) allow_builds: Option<BTreeMap<String, Value>>,
    // Preserve any keys this struct does not model (overrides, catalogs,
    // blockExoticSubdeps, ...) so re-emitting the file never clobbers
    // consumer or tool-added configuration.
    #[serde(flatten)]
    pub(crate) other: BTreeMap<String, Value>,
}

fn default_allow_builds() -> BTreeMap<String, Value> {
    ALLOWED_BUILD_DEPENDENCIES
        .iter()
        .map(|dep| ((*dep).to_string(), Value::Bool(true)))
        .collect()
}

/// pnpm writes string placeholders ("set this to true or false") for
/// unapproved build deps. Coerce any non-boolean entries to `true` and make
/// sure the scaffold's known build deps are present, healing workspaces that
/// picked up placeholders before this fix.
fn sanitize_allow_builds(pnpm_workspace: &mut PnpmWorkspace) {
    let mut allow_builds = pnpm_workspace
        .allow_builds
        .take()
        .unwrap_or_else(default_allow_builds);
    for value in allow_builds.values_mut() {
        if !matches!(value, Value::Bool(_)) {
            *value = Value::Bool(true);
        }
    }
    for dep in ALLOWED_BUILD_DEPENDENCIES {
        allow_builds
            .entry((*dep).to_string())
            .or_insert(Value::Bool(true));
    }
    pnpm_workspace.allow_builds = Some(allow_builds);
}

pub(crate) fn generate_pnpm_workspace(
    application_path: &str,
    additional_projects: &Vec<ProjectEntry>,
) -> Result<Option<RenderedTemplate>> {
    let pnpm_workspace_path = Path::new(application_path).join("pnpm-workspace.yaml");
    if pnpm_workspace_path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path: pnpm_workspace_path,
        content: to_string(&PnpmWorkspace {
            packages: additional_projects.iter().map(|p| p.name.clone()).collect(),
            allow_builds: Some(default_allow_builds()),
            other: BTreeMap::new(),
        })
        .with_context(|| ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE)?,
        context: None,
    }))
}

/// Render a pnpm-workspace.yaml with the given package list, preserving any
/// existing configuration (allowBuilds, overrides, ...) from the file at
/// `base_path` when present.
pub(crate) fn render_pnpm_workspace_with_packages(
    base_path: &Path,
    packages: Vec<String>,
) -> Result<String> {
    let pnpm_workspace_path = base_path.join("pnpm-workspace.yaml");
    let mut pnpm_workspace: PnpmWorkspace = if pnpm_workspace_path.exists() {
        from_str(
            &read_to_string(&pnpm_workspace_path)
                .with_context(|| ERROR_FAILED_TO_READ_PNPM_WORKSPACE)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_PNPM_WORKSPACE)?
    } else {
        PnpmWorkspace {
            packages: Vec::new(),
            allow_builds: None,
            other: BTreeMap::new(),
        }
    };
    pnpm_workspace.packages = packages;
    sanitize_allow_builds(&mut pnpm_workspace);
    Ok(to_string(&pnpm_workspace)
        .with_context(|| ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE)?)
}

pub(crate) fn add_project_definition_to_pnpm_workspace<
    T: ManifestConfig + ProjectManifestConfig + InitializableManifestConfig + Serialize,
>(
    base_path: &Path,
    manifest_data: &T,
) -> Result<String> {
    let pnpm_workspace_path = base_path.join("pnpm-workspace.yaml");
    let mut pnpm_workspace: PnpmWorkspace = from_str(
        &read_to_string(&pnpm_workspace_path)
            .with_context(|| ERROR_FAILED_TO_READ_PNPM_WORKSPACE)?,
    )
    .with_context(|| ERROR_FAILED_TO_PARSE_PNPM_WORKSPACE)?;
    if !pnpm_workspace.packages.contains(&manifest_data.name()) {
        pnpm_workspace.packages.push(manifest_data.name().clone());
    }
    sanitize_allow_builds(&mut pnpm_workspace);
    Ok(to_string(&pnpm_workspace)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE)?)
}

pub(crate) fn remove_project_definition_to_pnpm_workspace(
    base_path: &Path,
    project_name: &str,
) -> Result<String> {
    let pnpm_workspace_path = base_path.join("pnpm-workspace.yaml");
    let mut pnpm_workspace: PnpmWorkspace = from_str(
        &read_to_string(&pnpm_workspace_path)
            .with_context(|| ERROR_FAILED_TO_READ_PNPM_WORKSPACE)?,
    )
    .with_context(|| ERROR_FAILED_TO_PARSE_PNPM_WORKSPACE)?;
    if let Some(position) = pnpm_workspace
        .packages
        .iter()
        .position(|name| name == project_name)
    {
        pnpm_workspace.packages.remove(position);
    }
    sanitize_allow_builds(&mut pnpm_workspace);

    Ok(to_string(&pnpm_workspace)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE)?)
}
