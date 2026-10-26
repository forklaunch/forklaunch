use std::{fs::read_to_string, io::Write, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{from_str as json_from_str, to_string_pretty};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_PARSE_PACKAGE_JSON, ERROR_FAILED_TO_READ_PACKAGE_JSON, SdkModeType,
    },
    core::{
        command::command,
        manifest::{ProjectEntry, ProjectType, application::ApplicationManifestData},
        package_json::project_package_json::ProjectPackageJson,
        removal_template::{RemovalTemplate, remove_template_files},
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        tsconfig::generate_root_tsconfig,
        vscode::generate_vscode_tasks,
    },
};

#[derive(Debug)]
pub(crate) struct ModeCommand;

impl ModeCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

/// Apply generated SDK mode setup (common logic for both init and change commands)
pub(crate) fn apply_generated_sdk_mode_setup(
    app_root_path: &PathBuf,
    manifest_data: &ApplicationManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    // Hybrid workspaces own Cargo/TypeScript orchestration together. Mode changes
    // affect TS package resolution, not their shared build/editor configuration.
    if !has_native_workspace(manifest_data) {
        if let Some(root_tsconfig) = generate_root_tsconfig(app_root_path, manifest_data)? {
            rendered_templates_cache.insert(
                app_root_path.join("tsconfig.json").to_string_lossy(),
                root_tsconfig,
            );
        }

        if let Some(vscode_tasks) = generate_vscode_tasks(app_root_path, manifest_data)? {
            rendered_templates_cache.insert(
                app_root_path
                    .join(".vscode")
                    .join("tasks.json")
                    .to_string_lossy(),
                vscode_tasks,
            );
        }
    }
    let modules_path = app_root_path.join(manifest_data.modules_path.clone());
    for project in &manifest_data.projects {
        if project.name == "client-sdk"
            || matches!(project.r#type, ProjectType::Library)
            || !is_typescript_project(manifest_data, project)
        {
            continue;
        }

        let package_json_path = modules_path.join(&project.name).join("package.json");

        // Try to get from cache first (for init), then from disk (for mode change)
        let content = if let Ok(Some(template)) = rendered_templates_cache.get(&package_json_path) {
            template.content.clone()
        } else if package_json_path.exists() {
            read_to_string(&package_json_path).with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
        } else {
            continue;
        };

        let mut package_json: ProjectPackageJson =
            json_from_str(&content).with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

        package_json.types = Some("index.d.ts".to_string());

        rendered_templates_cache.insert(
            package_json_path.to_string_lossy(),
            RenderedTemplate {
                path: package_json_path.clone(),
                content: to_string_pretty(&package_json)?,
                context: None,
            },
        );
    }

    Ok(())
}

pub(crate) fn use_generated_sdk_mode(
    app_root_path: &PathBuf,
    manifest_data: &ApplicationManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    apply_generated_sdk_mode_setup(app_root_path, manifest_data, rendered_templates_cache)
}

fn use_live_sdk_mode(
    app_root_path: &PathBuf,
    manifest_data: &ApplicationManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<Vec<RemovalTemplate>> {
    ensure_live_sdk_mode_supported(manifest_data)?;

    let mut removal_templates = vec![];

    if !has_native_workspace(manifest_data) {
        removal_templates.push(RemovalTemplate {
            path: app_root_path.join("tsconfig.json"),
        });

        removal_templates.push(RemovalTemplate {
            path: app_root_path.join(".vscode").join("tasks.json"),
        });
    }

    let modules_path = app_root_path.join(manifest_data.modules_path.clone());
    for project in &manifest_data.projects {
        if project.name == "client-sdk"
            || matches!(project.r#type, ProjectType::Library)
            || !is_typescript_project(manifest_data, project)
        {
            continue;
        }

        let package_json_path = modules_path.join(&project.name).join("package.json");
        if package_json_path.exists() {
            let content = read_to_string(&package_json_path)
                .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?;
            let mut package_json: ProjectPackageJson =
                json_from_str(&content).with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

            package_json.types = None;

            rendered_templates_cache.insert(
                package_json_path.to_string_lossy(),
                RenderedTemplate {
                    path: package_json_path.clone(),
                    content: to_string_pretty(&package_json)?,
                    context: None,
                },
            );
        }
    }

    Ok(removal_templates)
}

fn has_native_workspace(manifest: &ApplicationManifestData) -> bool {
    matches!(
        manifest
            .language
            .as_deref()
            .or(manifest.application_language.as_deref()),
        Some("rust" | "mixed")
    ) || manifest.runtime == "tokio"
        || manifest.cargo_workspace.is_some()
        || manifest.projects.iter().any(|project| {
            project
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.language.as_deref())
                == Some("rust")
        })
}

fn is_typescript_project(manifest: &ApplicationManifestData, project: &ProjectEntry) -> bool {
    match project
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.language.as_deref())
    {
        Some("rust") => false,
        Some("typescript") => true,
        _ => {
            !matches!(
                manifest
                    .language
                    .as_deref()
                    .or(manifest.application_language.as_deref()),
                Some("rust" | "mixed")
            ) && manifest.runtime != "tokio"
        }
    }
}

fn ensure_live_sdk_mode_supported(manifest: &ApplicationManifestData) -> Result<()> {
    if has_native_workspace(manifest)
        && !manifest
            .projects
            .iter()
            .any(|project| project.name != "client-sdk" && is_typescript_project(manifest, project))
    {
        bail!(
            "Live SDK mode requires a TypeScript project. Rust services retain generated SDKs from contract artifacts."
        );
    }
    Ok(())
}

impl CliCommand for ModeCommand {
    fn command(&self) -> Command {
        command("mode", "Manage SDK modes")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("type")
                    .short('t')
                    .long("type")
                    .value_parser(SdkModeType::VARIANTS)
                    .required(true)
                    .help("The type of the mode"),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, existing_manifest_data) =
            crate::core::validate::require_manifest(matches)?;

        let mode_type = matches.get_one::<String>("type");
        let dryrun = matches.get_flag("dryrun");

        let mode_type = mode_type.unwrap().parse::<SdkModeType>()?;

        let mut rendered_templates_cache = RenderedTemplatesCache::new();
        let mut removal_templates: Vec<RemovalTemplate> = vec![];

        match mode_type {
            SdkModeType::Generated => use_generated_sdk_mode(
                &app_root_path,
                &existing_manifest_data,
                &mut rendered_templates_cache,
            )?,
            SdkModeType::Live => {
                removal_templates.extend(use_live_sdk_mode(
                    &app_root_path,
                    &existing_manifest_data,
                    &mut rendered_templates_cache,
                )?);
            }
        }

        let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;
        remove_template_files(&removal_templates, dryrun, &mut stdout)?;

        if !dryrun {
            log_ok!(stdout, "SDK mode changed successfully!");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn sdk_modes_preserve_library_compiler_entrypoints() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().to_path_buf();
        let manifest: ApplicationManifestData = serde_json::from_value(json!({
            "id":"fixture","cli_version":"0.0.0","app_name":"fixture",
            "app_description":"fixture","linter":"eslint","formatter":"prettier",
            "validator":"zod","http_framework":"express","author":"fixture","license":"MIT",
            "project_peer_topology":{},"runtime":"node","modules_path":"modules",
            "projects":[{"type":"Library","name":"core","description":"fixture"}]
        }))
        .unwrap();
        let package = path.join("modules/core/package.json");
        std::fs::create_dir_all(package.parent().unwrap()).unwrap();
        let original = r#"{"name":"core","main":"lib/index.js","types":"lib/index.d.ts"}"#;
        std::fs::write(&package, original).unwrap();
        let mut cache = RenderedTemplatesCache::new();
        apply_generated_sdk_mode_setup(&path, &manifest, &mut cache).unwrap();
        assert_eq!(cache.get(&package).unwrap().unwrap().content, original);
        use_live_sdk_mode(&path, &manifest, &mut cache).unwrap();
        assert_eq!(cache.get(&package).unwrap().unwrap().content, original);
        assert_eq!(std::fs::read_to_string(package).unwrap(), original);
    }

    #[test]
    fn legacy_typescript_modes_keep_package_and_configuration_behavior() {
        for runtime in ["node", "bun"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().to_path_buf();
            let manifest: ApplicationManifestData = serde_json::from_value(json!({
                "id":"fixture","cli_version":"0.0.0","app_name":"fixture",
                "app_description":"fixture","linter":"eslint","formatter":"prettier",
                "validator":"zod","http_framework":"express","author":"fixture","license":"MIT",
                "project_peer_topology":{},"runtime":runtime,"modules_path":"modules",
                "projects":[{"type":"Service","name":"api","description":"fixture"}]
            }))
            .unwrap();
            let package = path.join("modules/api/package.json");
            std::fs::create_dir_all(package.parent().unwrap()).unwrap();
            std::fs::write(&package, r#"{"name":"api","types":"index.d.ts"}"#).unwrap();
            let mut cache = RenderedTemplatesCache::new();
            let removals = use_live_sdk_mode(&path, &manifest, &mut cache).unwrap();
            assert_eq!(removals.len(), 2);
            let value: serde_json::Value =
                serde_json::from_str(&cache.get(&package).unwrap().unwrap().content).unwrap();
            assert!(value.get("types").is_none_or(serde_json::Value::is_null));
            apply_generated_sdk_mode_setup(&path, &manifest, &mut cache).unwrap();
            assert!(cache.get(path.join("tsconfig.json")).unwrap().is_some());
            assert!(
                cache
                    .get(path.join(".vscode/tasks.json"))
                    .unwrap()
                    .is_some()
            );
            let value: serde_json::Value =
                serde_json::from_str(&cache.get(&package).unwrap().unwrap().content).unwrap();
            assert_eq!(value["types"], "index.d.ts");
        }
    }
}
