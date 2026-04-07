use std::{collections::HashSet, fs::read_to_string, path::Path};

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_yml::{from_value, to_value};
use termcolor::StandardStream;
use walkdir::WalkDir;

use super::clean_application::clean_application;
use crate::{
    constants::Runtime,
    core::{
        client_sdk::{change_project_in_client_sdk, regenerate_client_sdk_compliance},
        docker::{
            Command, DependsOn, DockerBuild, DockerCompose, DockerService, HealthTest,
            Healthcheck,
        },
        manifest::{MutableManifestData, ProjectEntry, ProjectType},
        move_template::MoveTemplate,
        package_json::{
            application_package_json::ApplicationPackageJson,
            project_package_json::ProjectPackageJson, replace_project_in_workspace_definition,
        },
        removal_template::RemovalTemplate,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache},
        string::short_circuit_replacement,
        tsconfig::update_project_in_modules_tsconfig,
    },
};

pub(crate) fn change_name_in_files(
    base_path: &Path,
    existing_name: &str,
    name: &str,
    runtime: &Runtime,
    confirm: bool,
    project_entry: &mut ProjectEntry,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<Vec<RemovalTemplate>> {
    clean_application(base_path.parent().unwrap(), runtime, confirm, stdout)?;

    let mut removal_templates = Vec::new();

    if let Some(routers) = &mut project_entry.routers {
        routers.iter_mut().for_each(|router| {
            if router == existing_name {
                *router = name.to_string();
            }
        });
    }

    let existing_camel_case_name = existing_name.to_case(Case::Camel);
    let existing_kebab_case_name = existing_name.to_case(Case::Kebab);
    let existing_pascal_case_name = existing_name.to_case(Case::Pascal);

    let camel_case_name = name.to_case(Case::Camel);
    let kebab_case_name = name.to_case(Case::Kebab);
    let pascal_case_name = name.to_case(Case::Pascal);

    let mut seen_existing_names = HashSet::new();
    let mut replacements = vec![(existing_pascal_case_name.clone(), pascal_case_name.clone())];
    seen_existing_names.insert(existing_pascal_case_name.clone());

    for (existing_replacement, new_replacement) in [
        (existing_camel_case_name.clone(), camel_case_name.clone()),
        (existing_kebab_case_name.clone(), kebab_case_name.clone()),
        (existing_name.to_string(), name.to_string()),
    ] {
        if !seen_existing_names.contains(&existing_replacement) {
            replacements.push((existing_replacement.clone(), new_replacement.clone()));
            seen_existing_names.insert(existing_replacement.clone());
        }
    }

    for entry in WalkDir::new(base_path) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            if entry.path().to_string_lossy().contains("node_modules")
                || entry.path().to_string_lossy().contains("dist")
                || entry.path().to_string_lossy().contains("lib")
            {
                continue;
            }
            let relative_path = entry.path().strip_prefix(base_path)?;

            let content =
                if let Some(template) = rendered_templates_cache.get(entry.path()).ok().unwrap() {
                    template.content.clone()
                } else {
                    read_to_string(entry.path())?
                };

            let new_content = short_circuit_replacement(&content, &replacements);
            let new_file_name = short_circuit_replacement(
                &relative_path.to_string_lossy().to_string(),
                &replacements,
            );

            let new_path = base_path.join(new_file_name.clone());
            rendered_templates_cache.insert(
                entry.path().to_string_lossy(),
                RenderedTemplate {
                    path: new_path.clone().into(),
                    content: new_content,
                    context: None,
                },
            );

            if relative_path.to_string_lossy().to_string() != new_file_name.clone() {
                removal_templates.push(RemovalTemplate {
                    path: entry.path().to_path_buf(),
                })
            }
        }
    }

    Ok(removal_templates)
}

pub(crate) fn change_name(
    base_path: &Path,
    name: &str,
    confirm: bool,
    manifest_data: MutableManifestData,
    application_package_json: &mut ApplicationPackageJson,
    project_package_json: &mut ProjectPackageJson,
    docker_compose: Option<&mut DockerCompose>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    removal_templates: &mut Vec<RemovalTemplate>,
    stdout: &mut StandardStream,
) -> Result<MoveTemplate> {
    let existing_name = base_path.file_name().unwrap().to_string_lossy().to_string();

    // TODO: change the name of the package in client-sdk if service or worker (could just be simple find/replace)
    let (runtime, app_name, projects, project_peer_topology) = match manifest_data {
        MutableManifestData::Service(manifest_data) => {
            manifest_data.service_name = name.to_string();
            (
                manifest_data.runtime.as_mut(),
                manifest_data.app_name.as_mut(),
                &mut manifest_data.projects,
                manifest_data.project_peer_topology.values_mut(),
            )
        }
        MutableManifestData::Library(manifest_data) => {
            manifest_data.library_name = name.to_string();
            (
                manifest_data.runtime.as_mut(),
                manifest_data.app_name.as_mut(),
                &mut manifest_data.projects,
                manifest_data.project_peer_topology.values_mut(),
            )
        }
        MutableManifestData::Router(manifest_data) => {
            manifest_data.router_name = name.to_string();
            (
                manifest_data.runtime.as_mut(),
                manifest_data.app_name.as_mut(),
                &mut manifest_data.projects,
                manifest_data.project_peer_topology.values_mut(),
            )
        }
        MutableManifestData::Worker(manifest_data) => {
            manifest_data.worker_name = name.to_string();
            (
                manifest_data.runtime.as_mut(),
                manifest_data.app_name.as_mut(),
                &mut manifest_data.projects,
                manifest_data.project_peer_topology.values_mut(),
            )
        }
        _ => bail!("Invalid manifest data"),
    };

    let _ = replace_project_in_workspace_definition(
        base_path.parent().unwrap(),
        application_package_json,
        &runtime.parse()?,
        &existing_name,
        name,
        rendered_templates_cache,
    )?;

    let project_entry = projects
        .iter_mut()
        .find(|project| project.name == existing_name)
        .unwrap();

    project_entry.name = name.to_string();

    project_peer_topology.for_each(|group| {
        group.iter_mut().for_each(|project| {
            if project == &existing_name {
                *project = name.to_string();
            }
        });
    });

    project_package_json.name = Some(format!("@{}/{}", app_name, name.to_string()));

    if let Some(docker_compose) = docker_compose {
        let new_services: IndexMap<String, DockerService> = docker_compose
            .services
            .iter_mut()
            .map(|(key, value)| {
                if key.contains(&existing_name) {
                    (
                        key.replace(&existing_name, &name),
                        DockerService {
                            hostname: value
                                .hostname
                                .as_ref()
                                .map(|hostname| hostname.replace(&existing_name, &name)),
                            container_name: value.container_name.as_ref().map(|container_name| {
                                container_name.replace(&existing_name, &name)
                            }),
                            image: value
                                .image
                                .as_ref()
                                .map(|image| image.replace(&existing_name, &name)),
                            restart: value.restart.clone(),
                            build: value.build.as_ref().map(|build| DockerBuild {
                                context: build.context.replace(&existing_name, &name),
                                dockerfile: build.dockerfile.replace(&existing_name, &name),
                            }),
                            environment: value.environment.as_ref().map(|environment| {
                                environment
                                    .iter()
                                    .map(|(key, value)| {
                                        (
                                            key.replace(&existing_name, &name),
                                            value.replace(&existing_name, &name),
                                        )
                                    })
                                    .collect()
                            }),
                            depends_on: value.depends_on.as_ref().map(|depends_on| {
                                depends_on
                                    .iter()
                                    .map(|(key, value)| {
                                        (
                                            key.replace(&existing_name, &name),
                                            DependsOn {
                                                condition: value.condition.clone(),
                                            },
                                        )
                                    })
                                    .collect()
                            }),
                            ports: value.ports.as_ref().map(|ports| {
                                ports
                                    .iter()
                                    .map(|port| port.replace(&existing_name, &name))
                                    .collect()
                            }),
                            volumes: value.volumes.as_ref().map(|volumes| {
                                volumes
                                    .iter()
                                    .map(|volume| volume.replace(&existing_name, &name))
                                    .collect()
                            }),
                            networks: value.networks.as_ref().map(|networks| {
                                networks
                                    .iter()
                                    .map(|network| network.replace(&existing_name, &name))
                                    .collect()
                            }),
                            working_dir: value
                                .working_dir
                                .as_ref()
                                .map(|working_dir| working_dir.replace(&existing_name, &name)),
                            command: value.command.as_ref().map(|command| match command {
                                Command::Simple(command) => {
                                    Command::Simple(command.replace(&existing_name, &name))
                                }
                                Command::Multiple(commands) => Command::Multiple(
                                    commands
                                        .iter()
                                        .map(|command| command.replace(&existing_name, &name))
                                        .collect(),
                                ),
                            }),
                            entrypoint: value.entrypoint.as_ref().map(|entrypoint| match entrypoint {
                                Command::Simple(s) => {
                                    Command::Simple(s.replace(&existing_name, &name))
                                }
                                Command::Multiple(args) => Command::Multiple(
                                    args
                                        .iter()
                                        .map(|arg| arg.replace(&existing_name, &name))
                                        .collect(),
                                ),
                            }),
                            healthcheck: value.healthcheck.as_ref().map(|healthcheck| {
                                Healthcheck {
                                    test: match &healthcheck.test {
                                        HealthTest::String(s) => {
                                            HealthTest::String(
                                                s.replace(&existing_name, &name),
                                            )
                                        }
                                        HealthTest::List(list) => {
                                            HealthTest::List(
                                                list.iter()
                                                    .map(|item| item.replace(&existing_name, &name))
                                                    .collect(),
                                            )
                                        }
                                    },
                                    interval: healthcheck.interval.replace(&existing_name, &name),
                                    timeout: healthcheck.timeout.replace(&existing_name, &name),
                                    retries: healthcheck.retries,
                                    start_period: healthcheck
                                        .start_period
                                        .replace(&existing_name, &name),
                                    additional_properties: healthcheck
                                        .additional_properties
                                        .iter()
                                        .map(|(key, value)| {
                                            (
                                                key.replace(&existing_name, &name),
                                                to_value(
                                                    from_value::<String>(value.clone())
                                                        .unwrap()
                                                        .replace(&existing_name, &name),
                                                )
                                                .unwrap(),
                                            )
                                        })
                                        .collect(),
                                }
                            }),
                            additional_properties: value
                                .additional_properties
                                .iter()
                                .map(|(key, value)| {
                                    (
                                        key.replace(&existing_name, &name),
                                        to_value(
                                            from_value::<String>(value.clone())
                                                .unwrap()
                                                .replace(&existing_name, &name),
                                        )
                                        .unwrap(),
                                    )
                                })
                                .collect(),
                        },
                    )
                } else {
                    (key.to_string(), value.clone())
                }
            })
            .collect();

        docker_compose.services = new_services;
    }

    removal_templates.extend(change_name_in_files(
        base_path,
        &existing_name,
        name,
        &runtime.parse()?,
        confirm,
        project_entry,
        rendered_templates_cache,
        stdout,
    )?);

    let project_changed_type = project_entry.r#type.clone();
    if project_changed_type == ProjectType::Service
        || project_changed_type == ProjectType::Worker
    {
        change_project_in_client_sdk(
            rendered_templates_cache,
            base_path.parent().unwrap(),
            &app_name,
            &existing_name,
            &name,
        )?;

        // project_entry borrow ends above; safe to re-borrow `projects` as &[_]
        regenerate_client_sdk_compliance(
            rendered_templates_cache,
            base_path.parent().unwrap(),
            projects,
        )?;
    }

    let tsconfig_update =
        update_project_in_modules_tsconfig(base_path.parent().unwrap(), &existing_name, &name)?;
    rendered_templates_cache.insert(
        tsconfig_update.path.to_string_lossy().to_string(),
        tsconfig_update,
    );

    Ok(MoveTemplate {
        path: base_path.to_path_buf(),
        target: base_path.parent().unwrap().join(name),
    })
}
