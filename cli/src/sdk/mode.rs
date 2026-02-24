use std::{fs::read_to_string, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{from_str as json_from_str, to_string_pretty};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_PARSE_PACKAGE_JSON,
        ERROR_FAILED_TO_READ_PACKAGE_JSON, SdkModeType,
    },
    core::{
        command::command,
        manifest::application::ApplicationManifestData,
        package_json::project_package_json::ProjectPackageJson,
        removal_template::{RemovalTemplate, RemovalTemplateType, remove_template_files},
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

    let modules_path = app_root_path.join(manifest_data.modules_path.clone());
    for project in &manifest_data.projects {
        if project.name == "client-sdk" {
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
    let mut removal_templates = vec![];

    removal_templates.push(RemovalTemplate {
        path: app_root_path.join("tsconfig.json"),
        r#type: RemovalTemplateType::File,
    });

    removal_templates.push(RemovalTemplate {
        path: app_root_path.join(".vscode").join("tasks.json"),
        r#type: RemovalTemplateType::File,
    });

    let modules_path = app_root_path.join(manifest_data.modules_path.clone());
    for project in &manifest_data.projects {
        if project.name == "client-sdk" {
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

impl CliCommand for ModeCommand {
    fn command(&self) -> Command {
        command("mode", "Manage SDK modes")
            .arg(
                Arg::new("type")
                    .short('t')
                    .long("type")
                    .value_parser(SdkModeType::VARIANTS)
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

        let (app_root_path, existing_manifest_data) = crate::core::validate::require_manifest(matches)?;

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
