use std::{
    fs::{read_to_string, remove_dir_all},
    io::Write,
};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use toml::to_string_pretty as toml_to_string_pretty;

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_CREATE_PACKAGE_JSON, ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE,
        ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE, ERROR_FAILED_TO_PARSE_MANIFEST,
        ERROR_FAILED_TO_READ_MANIFEST, ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE,
        ERROR_FAILED_TO_WRITE_MANIFEST, Runtime,
    },
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        client_sdk::{regenerate_client_sdk_compliance, remove_project_from_client_sdk},
        command::command,
        docker::{DockerCompose, remove_service_from_docker_compose},
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType,
            application::ApplicationManifestData, remove_project_definition_from_manifest,
        },
        package_json::remove_project_definition_to_package_json,
        pnpm_workspace::remove_project_definition_to_pnpm_workspace,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        tsconfig::remove_project_from_modules_tsconfig,
    },
    prompt::{ArrayCompleter, prompt_for_confirmation, prompt_with_validation},
};

#[derive(Debug)]
pub(super) struct ServiceCommand;

impl ServiceCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for ServiceCommand {
    fn command(&self) -> Command {
        command("service", "Initialize a new service")
            .alias("svc")
            .arg(Arg::new("name").help("The name of the service"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path to initialize the library in"),
            )
            .arg(
                Arg::new("continue")
                    .short('c')
                    .long("continue")
                    .help("Continue the eject operation")
                    .action(ArgAction::SetTrue),
            )
    }

    // pass token in from parent and perform get token above?
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data = toml::from_str::<ApplicationManifestData>(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let service_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            "service name",
            None,
            |input: &str| {
                existing_manifest_data
                    .projects
                    .iter()
                    .any(|project| project.name == input && project.r#type == ProjectType::Service)
            },
            |_| "Service not found".to_string(),
        )?;

        let service_base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Application(&existing_manifest_data),
            &None,
            &mut line_editor,
            &mut stdout,
            matches,
            0,
        )?;

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Application(ApplicationInitializationMetadata {
                app_name: app_root_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                database: match existing_manifest_data
                    .projects
                    .iter()
                    .find(|project| {
                        project.name == service_name && project.r#type == ProjectType::Service
                    })
                    .map(|project| project.resources.as_ref().unwrap())
                {
                    Some(resource_inventory) => resource_inventory.database.clone(),
                    None => None,
                },
            }),
        );

        let continue_delete_override = matches.get_flag("continue");

        if !continue_delete_override {
            let continue_delete = prompt_for_confirmation(
                &mut line_editor,
                "This operation is irreversible. Do you want to continue? (y/N) ",
            )?;

            if !continue_delete {
                log_error!(stdout, "Deletion cancelled");
                return Ok(());
            }
        }

        remove_project_definition_from_manifest(&mut manifest_data, &service_name)?;

        remove_dir_all(&service_base_path.join(&service_name))?;

        let docker_compose_path =
            if let Some(docker_compose_path) = &manifest_data.docker_compose_path {
                app_root_path.join(docker_compose_path)
            } else {
                app_root_path.join("docker-compose.yaml")
            };

        let mut rendered_templates_cache = RenderedTemplatesCache::new();

        rendered_templates_cache.get(&manifest_path)?;
        rendered_templates_cache.get(&docker_compose_path)?;

        rendered_templates_cache.insert(
            manifest_path.to_string_lossy().to_string(),
            RenderedTemplate {
                path: manifest_path.clone(),
                content: toml_to_string_pretty(&manifest_data)
                    .context(ERROR_FAILED_TO_WRITE_MANIFEST)?,
                context: Some(ERROR_FAILED_TO_WRITE_MANIFEST.to_string()),
            },
        );

        let docker_compose_template = rendered_templates_cache.get(&docker_compose_path)?.unwrap();
        let mut docker_compose: DockerCompose =
            serde_yml::from_str(&docker_compose_template.content)
                .with_context(|| ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE)?;
        remove_service_from_docker_compose(&mut docker_compose, &service_name)?;

        rendered_templates_cache.insert(
            docker_compose_path.to_string_lossy().to_string(),
            RenderedTemplate {
                path: docker_compose_path.clone(),
                content: serde_yml::to_string(&docker_compose)?,
                context: Some(ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE.to_string()),
            },
        );

        match manifest_data.runtime.parse()? {
            Runtime::Node => {
                let pnpm_workspace_path = service_base_path.join("pnpm-workspace.yaml");
                rendered_templates_cache.get(&pnpm_workspace_path)?;

                rendered_templates_cache.insert(
                    pnpm_workspace_path.to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: pnpm_workspace_path,
                        content: remove_project_definition_to_pnpm_workspace(
                            &service_base_path,
                            &service_name,
                        )?,
                        context: Some(ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE.to_string()),
                    },
                );
            }
            Runtime::Bun => {
                let package_json_path = service_base_path.join("package.json");
                rendered_templates_cache.get(&package_json_path)?;

                rendered_templates_cache.insert(
                    package_json_path.to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: package_json_path,
                        content: remove_project_definition_to_package_json(
                            &service_base_path,
                            &service_name,
                        )?,
                        context: Some(ERROR_FAILED_TO_CREATE_PACKAGE_JSON.to_string()),
                    },
                );
            }
        }

        remove_project_from_client_sdk(
            &mut rendered_templates_cache,
            &service_base_path,
            &manifest_data.app_name,
            &service_name,
        )?;

        remove_project_from_client_sdk(
            &mut rendered_templates_cache,
            &service_base_path,
            &manifest_data.app_name,
            &service_name,
        )?;

        regenerate_client_sdk_compliance(
            &mut rendered_templates_cache,
            &service_base_path,
            &manifest_data.projects,
        )?;

        let tsconfig_template =
            remove_project_from_modules_tsconfig(&service_base_path, &service_name)
                .with_context(|| "Failed to remove service from modules tsconfig.json")?;
        rendered_templates_cache.insert(
            tsconfig_template.path.to_string_lossy().to_string(),
            tsconfig_template,
        );

        let rendered_templates: Vec<_> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();
        write_rendered_templates(&rendered_templates, false, &mut stdout)?;

        log_ok!(stdout, "{} deleted successfully!", service_name);

        Ok(())
    }
}
