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

/// Tooling (e.g. Studio upgrade flows) writes version-pinned entries like
/// `@forklaunch/core@1.5.9` into `minimumReleaseAgeExclude`. After a
/// framework release those pins go stale: pnpm blocks the new version with
/// ERR_PNPM_MINIMUM_RELEASE_AGE_VIOLATION and in-range resolution falls back
/// to the old one. First-party packages should always be exempt from the age
/// gate, so strip version suffixes down to bare package names on every
/// rewrite.
fn sanitize_minimum_release_age_exclude(pnpm_workspace: &mut PnpmWorkspace) {
    let Some(Value::Sequence(entries)) =
        pnpm_workspace.other.get_mut("minimumReleaseAgeExclude")
    else {
        return;
    };
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut normalized: Vec<Value> = Vec::new();
    for entry in entries.iter() {
        match entry {
            Value::String(spec) if spec.starts_with("@forklaunch/") => {
                // find a version separator after the scope's leading '@'
                let name = match spec[1..].find('@') {
                    Some(index) => spec[..index + 1].to_string(),
                    None => spec.clone(),
                };
                if seen.insert(name.clone()) {
                    normalized.push(Value::String(name));
                }
            }
            Value::String(spec) => {
                if seen.insert(spec.clone()) {
                    normalized.push(entry.clone());
                }
            }
            _ => normalized.push(entry.clone()),
        }
    }
    *entries = normalized;
}

/// The scope pattern that exempts every first-party package from the age gate.
const FORKLAUNCH_RELEASE_AGE_EXCLUDE: &str = "@forklaunch/*";

/// Add the first-party exemption, rather than only normalising entries that
/// happen to already be there.
///
/// `sanitize_minimum_release_age_exclude` states the rule -- first-party
/// packages should always be exempt -- but it can only rewrite a list that
/// exists. A freshly generated app has no `minimumReleaseAgeExclude` at all,
/// just the `minimumReleaseAge: 1440` default, so for the first 24 hours after
/// a framework release every `@forklaunch/*` version the CLI pins is younger
/// than the gate and `pnpm install` fails outright:
///
///     No version matching "@forklaunch/core" found for specifier "~1.5.17"
///     (blocked by minimum-release-age: 86400 seconds)
///
/// That is a release-day trap rather than a real supply-chain risk: the gate
/// exists to slow down compromised third-party publishes, and these are our
/// own packages, pinned by the CLI to versions it was built against. A single
/// scope pattern covers packages published later without needing another
/// entry here.
fn ensure_minimum_release_age_exclude(pnpm_workspace: &mut PnpmWorkspace) {
    let entry = pnpm_workspace
        .other
        .entry("minimumReleaseAgeExclude".to_string())
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let Value::Sequence(entries) = entry else {
        return;
    };
    let already_covered = entries.iter().any(|value| {
        matches!(value, Value::String(spec) if spec == FORKLAUNCH_RELEASE_AGE_EXCLUDE)
    });
    if !already_covered {
        entries.push(Value::String(FORKLAUNCH_RELEASE_AGE_EXCLUDE.to_string()));
    }
}

/// Set-if-missing on every rewrite path, not just initial creation, so an
/// app that already existed before this default was introduced (or had the
/// key stripped) still gets it back.
fn ensure_minimum_release_age(pnpm_workspace: &mut PnpmWorkspace) {
    pnpm_workspace
        .other
        .entry("minimumReleaseAge".to_string())
        .or_insert(Value::Number(1440.into()));
}

/// Pin explicitly rather than leave unset: pnpm >=11.2 defaults
/// minimumReleaseAge to 1440 (24h), but a scaffold's local `pnpm install`
/// may run on an older pnpm with no age gate at all, resolving versions the
/// deploy pipeline's newer pnpm then rejects from an already-locked
/// lockfile. Setting it explicitly makes local resolution skip too-new
/// versions the same way deploy does, so the generated lockfile is already
/// compliant. Same value as forklaunch-platform's own pnpm-workspace.yaml.
///
/// The first-party exemption ships with the default rather than being applied
/// afterwards, because `generate_pnpm_workspace` -- the initial-creation path --
/// serialises this map directly without running the `ensure_*` helpers. Leaving
/// it out there is what makes a newly scaffolded app fail to install for the
/// first 24 hours after a framework release.
fn default_other() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("minimumReleaseAge".to_string(), Value::Number(1440.into())),
        (
            "minimumReleaseAgeExclude".to_string(),
            Value::Sequence(vec![Value::String(
                FORKLAUNCH_RELEASE_AGE_EXCLUDE.to_string(),
            )]),
        ),
    ])
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
            other: default_other(),
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
            other: default_other(),
        }
    };
    pnpm_workspace.packages = packages;
    sanitize_allow_builds(&mut pnpm_workspace);
    sanitize_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age(&mut pnpm_workspace);
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
    sanitize_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age(&mut pnpm_workspace);
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
    sanitize_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age_exclude(&mut pnpm_workspace);
    ensure_minimum_release_age(&mut pnpm_workspace);

    Ok(to_string(&pnpm_workspace)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimum_release_age_exclude_strips_forklaunch_versions() {
        let mut ws: PnpmWorkspace = from_str(
            "packages:\n- core\nminimumReleaseAgeExclude:\n- '@forklaunch/core@1.5.9'\n- '@forklaunch/express@1.2.37'\n- '@forklaunch/core@1.5.10'\n- 'left-pad@1.3.0'\n",
        )
        .unwrap();
        sanitize_minimum_release_age_exclude(&mut ws);
        let rendered = to_string(&ws).unwrap();
        assert!(rendered.contains("- '@forklaunch/core'"));
        assert!(rendered.contains("- '@forklaunch/express'"));
        // duplicates collapse, non-forklaunch entries untouched
        assert_eq!(rendered.matches("@forklaunch/core").count(), 1);
        assert!(rendered.contains("left-pad@1.3.0"));
    }

    #[test]
    fn test_generated_workspace_exempts_forklaunch_from_the_age_gate() {
        // Covers the initial-creation path specifically: it serialises
        // default_other() directly and runs none of the ensure_* helpers, so
        // the exemption has to be in the default itself.
        let dir = std::env::temp_dir().join("fl-pnpm-ws-generate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let rendered = generate_pnpm_workspace(dir.to_str().unwrap(), &Vec::new())
            .unwrap()
            .expect("a fresh directory should produce a workspace file");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            rendered.content.contains("minimumReleaseAgeExclude"),
            "generated workspace must carry the exclude list: {}",
            rendered.content
        );
        assert!(
            rendered.content.contains("@forklaunch/*"),
            "generated workspace must exempt the first-party scope: {}",
            rendered.content
        );
        assert!(
            rendered.content.contains("minimumReleaseAge"),
            "the age gate itself must still be set: {}",
            rendered.content
        );
    }

    #[test]
    fn test_forklaunch_scope_is_exempted_when_no_list_exists() {
        // The release-day case: a fresh app has minimumReleaseAge but no
        // exclude list, so every just-published @forklaunch/* version is
        // younger than the gate and `pnpm install` fails outright.
        let mut ws: PnpmWorkspace = from_str("packages:\n- core\n").unwrap();
        ensure_minimum_release_age_exclude(&mut ws);

        let Some(Value::Sequence(entries)) = ws.other.get("minimumReleaseAgeExclude") else {
            panic!("expected the exclude list to be created");
        };
        assert_eq!(entries, &vec![Value::String("@forklaunch/*".to_string())]);
    }

    #[test]
    fn test_forklaunch_scope_is_not_duplicated() {
        let mut ws: PnpmWorkspace =
            from_str("packages:\n- core\nminimumReleaseAgeExclude:\n- '@forklaunch/*'\n")
                .unwrap();
        ensure_minimum_release_age_exclude(&mut ws);
        ensure_minimum_release_age_exclude(&mut ws);

        let Some(Value::Sequence(entries)) = ws.other.get("minimumReleaseAgeExclude") else {
            panic!("expected the exclude list to survive");
        };
        assert_eq!(entries.len(), 1, "rewrites must be idempotent: {entries:?}");
    }

    #[test]
    fn test_third_party_exclusions_are_preserved() {
        // The scope pattern is added alongside whatever a consumer put there;
        // it must never replace their entries.
        let mut ws: PnpmWorkspace =
            from_str("packages:\n- core\nminimumReleaseAgeExclude:\n- 'some-vendor-pkg'\n")
                .unwrap();
        ensure_minimum_release_age_exclude(&mut ws);

        let Some(Value::Sequence(entries)) = ws.other.get("minimumReleaseAgeExclude") else {
            panic!("expected the exclude list");
        };
        assert!(entries.contains(&Value::String("some-vendor-pkg".to_string())));
        assert!(entries.contains(&Value::String("@forklaunch/*".to_string())));
    }

    #[test]
    fn test_minimum_release_age_exclude_absent_is_noop() {
        let mut ws: PnpmWorkspace = from_str("packages:\n- core\n").unwrap();
        sanitize_minimum_release_age_exclude(&mut ws);
        assert!(!to_string(&ws).unwrap().contains("minimumReleaseAgeExclude"));
    }

    #[test]
    fn test_generate_pnpm_workspace_pins_minimum_release_age() {
        let dir = tempfile::tempdir().unwrap();
        let rendered = generate_pnpm_workspace(dir.path().to_str().unwrap(), &Vec::new())
            .unwrap()
            .unwrap();
        assert!(rendered.content.contains("minimumReleaseAge: 1440"));
    }

    #[test]
    fn test_render_pnpm_workspace_with_packages_pins_minimum_release_age_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let rendered =
            render_pnpm_workspace_with_packages(dir.path(), vec!["core".to_string()]).unwrap();
        assert!(rendered.contains("minimumReleaseAge: 1440"));
    }

    #[test]
    fn test_ensure_minimum_release_age_backfills_when_absent() {
        let mut ws: PnpmWorkspace = from_str("packages:\n- core\n").unwrap();
        ensure_minimum_release_age(&mut ws);
        assert!(to_string(&ws).unwrap().contains("minimumReleaseAge: 1440"));
    }

    #[test]
    fn test_ensure_minimum_release_age_preserves_existing_value() {
        let mut ws: PnpmWorkspace =
            from_str("packages:\n- core\nminimumReleaseAge: 0\n").unwrap();
        ensure_minimum_release_age(&mut ws);
        assert!(to_string(&ws).unwrap().contains("minimumReleaseAge: 0"));
    }
}
