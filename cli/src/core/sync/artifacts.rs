use std::{io::Write, path::Path};

use anyhow::{Context, Result};
use serde_json::{from_str as json_from_str, to_string_pretty as json_to_string_pretty};
use serde_yml::{from_str as yaml_from_str, to_string as yaml_to_string};
use termcolor::{StandardStream, WriteColor};
use toml::{from_str as toml_from_str, to_string_pretty as toml_to_string_pretty};

use crate::{
    constants::{
        Database, ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE,
        ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE, ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE, Infrastructure,
        Runtime, WorkerType,
    },
    core::{
        client_sdk::remove_project_from_client_sdk,
        docker::{
            DockerCompose, add_service_definition_to_docker_compose,
            add_worker_definition_to_docker_compose, remove_service_from_docker_compose,
            remove_worker_from_docker_compose,
        },
        manifest::{
            InitializableManifestConfig, InitializableManifestConfigMetadata, ProjectEntry,
            ProjectInitializationMetadata, ProjectMetadata, ProjectType, ResourceInventory,
            application::ApplicationManifestData, remove_project_definition_from_manifest,
            service::ServiceManifestData, worker::WorkerManifestData,
        },
        package_json::{
            application_package_json::ApplicationPackageJson,
            project_package_json::ProjectPackageJson,
        },
        pnpm_workspace::PnpmWorkspace,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache},
        sync::detection::detect_routers_from_service,
        tsconfig::add_project_to_modules_tsconfig,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactType {
    Manifest,
    DockerCompose,
    Runtime,
    ClientSdk,
    ModulesTsconfig,
}

#[derive(Clone, Debug)]
pub struct ProjectSyncMetadata {
    pub project_type: ProjectType,
    pub project_name: String,
    pub description: String,
    pub database: Option<Database>,
    pub infrastructure: Vec<Infrastructure>,
    pub worker_type: Option<WorkerType>,
}

impl ProjectSyncMetadata {
    fn to_resource_inventory(&self) -> Option<ResourceInventory> {
        let database = self.database.as_ref().map(|d| d.to_string());

        let cache = self.infrastructure.iter().find_map(|infra| match infra {
            Infrastructure::Redis => Some(Infrastructure::Redis.metadata().id.to_string()),
            Infrastructure::S3 => None,
        });

        let queue = self.worker_type.and_then(|wt| match wt {
            WorkerType::Kafka => Some(wt.to_string()),
            WorkerType::Database | WorkerType::RedisCache | WorkerType::BullMQCache => None,
        });

        let object_store = self.infrastructure.iter().find_map(|infra| match infra {
            Infrastructure::S3 => Some(Infrastructure::S3.metadata().id.to_string()),
            Infrastructure::Redis => None,
        });

        if database.is_some() || cache.is_some() || queue.is_some() || object_store.is_some() {
            Some(ResourceInventory {
                database,
                cache,
                queue,
                object_store,
                redis_partition: None,
            })
        } else {
            None
        }
    }

    fn to_project_metadata(&self) -> Option<ProjectMetadata> {
        self.worker_type.map(|wt| ProjectMetadata {
            r#type: Some(wt.to_string()),
        })
    }
}

/// Sync a project to multiple artifacts using RenderedTemplatesCache
///
/// This is the core sync function that implements the pattern:
/// - Check if project is present in artifacts
/// - Add if not present
/// - Save changes back to cache
///
/// Matches the pattern used in change command.
pub fn sync_project_to_artifacts(
    cache: &mut RenderedTemplatesCache,
    manifest_data: &mut ApplicationManifestData,
    metadata: &ProjectSyncMetadata,
    artifacts: &[ArtifactType],
    app_root_path: &Path,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    for artifact_type in artifacts {
        match artifact_type {
            ArtifactType::Manifest => {
                sync_to_manifest(manifest_data, metadata, modules_path, stdout)?;
            }
            ArtifactType::DockerCompose => {
                sync_to_docker_compose(manifest_data, cache, metadata, app_root_path, stdout)?;
            }
            ArtifactType::Runtime => {
                sync_to_runtime(manifest_data, cache, metadata, modules_path, stdout)?;
            }
            ArtifactType::ClientSdk => {
                sync_to_client_sdk(manifest_data, cache, metadata, modules_path, stdout)?;
            }
            ArtifactType::ModulesTsconfig => {
                sync_to_modules_tsconfig(cache, metadata, modules_path, stdout)?;
            }
        }
    }

    Ok(())
}

fn sync_to_manifest(
    manifest_data: &mut ApplicationManifestData,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    if manifest_data
        .projects
        .iter()
        .any(|p| p.name == metadata.project_name)
    {
        log_info!(stdout, "Already in manifest: {}", metadata.project_name);
        return Ok(());
    }

    let routers = detect_routers_for_project(
        &metadata.project_type,
        &metadata.project_name,
        modules_path,
        stdout,
    )?;

    manifest_data.projects.push(ProjectEntry {
        name: metadata.project_name.clone(),
        r#type: metadata.project_type.clone(),
        description: metadata.description.clone(),
        variant: None,
        resources: metadata.to_resource_inventory(),
        routers,
        metadata: metadata.to_project_metadata(),
    });

    manifest_data
        .project_peer_topology
        .entry(manifest_data.app_name.clone())
        .or_insert_with(Vec::new)
        .push(metadata.project_name.clone());

    log_ok!(stdout, "Added to manifest: {}", metadata.project_name);

    Ok(())
}

/// Detect routers for a project based on its type
fn detect_routers_for_project(
    project_type: &ProjectType,
    project_name: &str,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<Option<Vec<String>>> {
    let routers = match project_type {
        ProjectType::Service | ProjectType::Worker => {
            let project_path = modules_path.join(project_name);
            let detected_routers = detect_routers_from_service(&project_path)?;

            if !detected_routers.is_empty() {
                log_ok!(stdout, "Detected {} router(s): {}", detected_routers.len(), detected_routers.join(", "));
                Some(detected_routers)
            } else {
                None
            }
        }
        ProjectType::Library => None,
    };

    Ok(routers)
}

/// Sync to docker-compose.yaml - delegated to existing functions
///
/// Docker sync is handled separately because it requires complex ServiceManifestData/WorkerManifestData
/// structures with 50+ template fields. The existing validation functions handle this complexity.
/// This function just checks presence - the actual addition is handled by the caller.
fn sync_to_docker_compose(
    manifest_data: &mut ApplicationManifestData,
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    app_root_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    if !matches!(
        metadata.project_type,
        ProjectType::Service | ProjectType::Worker
    ) {
        return Ok(());
    }

    let docker_path = app_root_path.join(
        manifest_data
            .docker_compose_path
            .clone()
            .unwrap_or("docker-compose.yaml".to_string()),
    );

    let template = cache
        .get(&docker_path)?
        .context("Docker compose file not found")?;

    let mut docker_compose_content = template.content.clone();

    let docker_compose: DockerCompose =
        yaml_from_str(&docker_compose_content).context(ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE)?;

    if docker_compose.services.contains_key(&metadata.project_name) {
        log_info!(stdout, "Already in docker-compose: {}", metadata.project_name);
        return Ok(());
    }

    match metadata.project_type {
        ProjectType::Service => {
            let service_manifest_data: &ServiceManifestData =
                &toml_from_str(&toml_to_string_pretty(manifest_data)?)?;
            let service_manifest_data = service_manifest_data.initialize(
                InitializableManifestConfigMetadata::Project(ProjectInitializationMetadata {
                    project_name: metadata.project_name.clone(),
                    database: Some(metadata.database.clone().unwrap()),
                    infrastructure: Some(metadata.infrastructure.clone()),
                    description: Some(metadata.description.clone()),
                    worker_type: metadata.worker_type.clone(),
                }),
            );
            docker_compose_content = add_service_definition_to_docker_compose(
                &service_manifest_data,
                app_root_path,
                Some(template.content),
            )?;
        }
        ProjectType::Worker => {
            let worker_manifest_data: &WorkerManifestData =
                &toml_from_str(&toml_to_string_pretty(manifest_data)?)?;
            let worker_manifest_data = worker_manifest_data.initialize(
                InitializableManifestConfigMetadata::Project(ProjectInitializationMetadata {
                    project_name: metadata.project_name.clone(),
                    database: Some(metadata.database.clone().unwrap()),
                    infrastructure: Some(metadata.infrastructure.clone()),
                    description: Some(metadata.description.clone()),
                    worker_type: metadata.worker_type.clone(),
                }),
            );
            docker_compose_content = add_worker_definition_to_docker_compose(
                &worker_manifest_data,
                app_root_path,
                Some(template.content),
            )?;
        }
        ProjectType::Library => (),
    };

    cache.insert(
        docker_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: docker_path.clone(),
            content: docker_compose_content,
            context: Some(ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE.to_string()),
        },
    );

    Ok(())
}

fn sync_to_runtime(
    manifest_data: &mut ApplicationManifestData,
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    let runtime: Runtime = manifest_data.runtime.parse()?;

    match runtime {
        Runtime::Bun => {
            sync_to_package_json(cache, metadata, modules_path, stdout)?;
        }
        Runtime::Node => {
            sync_to_pnpm_workspace(cache, metadata, modules_path, stdout)?;
        }
    }

    Ok(())
}

fn sync_to_package_json(
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    let pkg_json_path = modules_path.join("package.json");

    let template = cache
        .get(&pkg_json_path)?
        .context("package.json not found")?;

    let mut pkg_json: ApplicationPackageJson =
        json_from_str(&template.content).context("Failed to parse package.json")?;

    let workspaces = pkg_json.workspaces.get_or_insert_with(Vec::new);
    if workspaces.contains(&metadata.project_name) {
        log_info!(stdout, "Already in package.json: {}", metadata.project_name);
        return Ok(());
    }

    workspaces.push(metadata.project_name.clone());

    cache.insert(
        pkg_json_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: pkg_json_path.clone(),
            content: json_to_string_pretty(&pkg_json)
                .context("Failed to serialize package.json")?,
            context: Some("Failed to write package.json".to_string()),
        },
    );

    log_ok!(stdout, "Added to package.json: {}", metadata.project_name);

    Ok(())
}

fn sync_to_client_sdk(
    manifest_data: &mut ApplicationManifestData,
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    if metadata.project_type != ProjectType::Service {
        return Ok(());
    }

    let sdk_ts_path = modules_path.join("client-sdk").join("clientSdk.ts");
    let sdk_json_path = modules_path.join("client-sdk").join("package.json");

    let ts_template = cache.get(&sdk_ts_path)?.context("clientSdk.ts not found")?;
    let json_template = cache
        .get(&sdk_json_path)?
        .context("SDK package.json not found")?;

    let mut sdk_pkg_json: ProjectPackageJson = json_from_str(&json_template.content)?;

    // Use existing SDK function (it handles the AST transformation)
    let (new_ts_content, new_pkg_json) = crate::core::client_sdk::add_project_vec_to_client_sdk(
        &manifest_data.app_name,
        &vec![metadata.project_name.clone()],
        &ts_template.content,
        &mut sdk_pkg_json,
    )?;

    cache.insert(
        sdk_ts_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_ts_path.clone(),
            content: new_ts_content,
            context: Some("Failed to write universal SDK".to_string()),
        },
    );

    cache.insert(
        sdk_json_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_json_path.clone(),
            content: json_to_string_pretty(&new_pkg_json)?,
            context: Some("Failed to write SDK package.json".to_string()),
        },
    );

    log_ok!(stdout, "Added to universal SDK: {}", metadata.project_name);

    Ok(())
}

fn sync_to_modules_tsconfig(
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    let rendered_template = add_project_to_modules_tsconfig(modules_path, &metadata.project_name)?;

    cache.insert(
        rendered_template.path.to_string_lossy().to_string(),
        rendered_template,
    );

    log_ok!(stdout, "Added to modules/tsconfig.json: {}", metadata.project_name);

    Ok(())
}

fn sync_to_pnpm_workspace(
    cache: &mut RenderedTemplatesCache,
    metadata: &ProjectSyncMetadata,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    let workspace_path = modules_path.join("pnpm-workspace.yaml");

    let template = cache
        .get(&workspace_path)?
        .context("pnpm-workspace.yaml not found")?;

    let mut workspace: PnpmWorkspace =
        yaml_from_str(&template.content).context("Failed to parse pnpm-workspace.yaml")?;

    if workspace.packages.contains(&metadata.project_name) {
        log_info!(stdout, "Already in pnpm-workspace: {}", metadata.project_name);
        return Ok(());
    }

    workspace.packages.push(metadata.project_name.clone());

    cache.insert(
        workspace_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: workspace_path.clone(),
            content: yaml_to_string(&workspace)
                .context(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE)?,
            context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE.to_string()),
        },
    );

    log_ok!(stdout, "Added to pnpm-workspace: {}", metadata.project_name);

    Ok(())
}

pub fn remove_project_from_artifacts(
    rendered_templates_cache: &mut RenderedTemplatesCache,
    manifest_data: &mut ApplicationManifestData,
    project_name: &str,
    project_type: ProjectType,
    artifact_types: &[ArtifactType],
    app_root_path: &Path,
    modules_path: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    for artifact_type in artifact_types {
        match artifact_type {
            ArtifactType::Manifest => {
                remove_project_definition_from_manifest(manifest_data, &project_name.to_string())?;
                log_ok!(stdout, "Removed from manifest");
            }
            ArtifactType::DockerCompose => {
                if matches!(project_type, ProjectType::Service | ProjectType::Worker) {
                    let docker_compose_path = app_root_path.join("docker-compose.yaml");

                    if docker_compose_path.exists() {
                        let docker_compose_template =
                            rendered_templates_cache.get(&docker_compose_path)?;
                        let mut docker_compose: DockerCompose =
                            if let Some(template) = docker_compose_template {
                                yaml_from_str(&template.content)
                                    .context("Failed to parse docker-compose")?
                            } else {
                                yaml_from_str(&std::fs::read_to_string(&docker_compose_path)?)
                                    .context("Failed to parse docker-compose")?
                            };

                        match project_type {
                            ProjectType::Service => {
                                remove_service_from_docker_compose(
                                    &mut docker_compose,
                                    project_name,
                                )?;
                            }
                            ProjectType::Worker => {
                                remove_worker_from_docker_compose(
                                    &mut docker_compose,
                                    project_name,
                                )?;
                            }
                            _ => {}
                        }

                        rendered_templates_cache.insert(
                            docker_compose_path.to_string_lossy(),
                            RenderedTemplate {
                                path: docker_compose_path.clone(),
                                content: yaml_to_string(&docker_compose)
                                    .context("Failed to serialize docker-compose")?,
                                context: None,
                            },
                        );

                        log_ok!(stdout, "Removed from docker-compose");
                    }
                }
            }
            ArtifactType::Runtime => {
                let runtime: Runtime = manifest_data.runtime.parse()?;

                match runtime {
                    Runtime::Node => {
                        let pnpm_workspace_path = modules_path.join("pnpm-workspace.yaml");
                        if pnpm_workspace_path.exists() {
                            let pnpm_template =
                                rendered_templates_cache.get(&pnpm_workspace_path)?;
                            let mut pnpm_workspace: PnpmWorkspace =
                                if let Some(template) = pnpm_template {
                                    yaml_from_str(&template.content)
                                        .context("Failed to parse pnpm-workspace")?
                                } else {
                                    yaml_from_str(&std::fs::read_to_string(&pnpm_workspace_path)?)
                                        .context("Failed to parse pnpm-workspace")?
                                };

                            if let Some(pos) = pnpm_workspace
                                .packages
                                .iter()
                                .position(|p| p == project_name)
                            {
                                pnpm_workspace.packages.remove(pos);
                            }

                            rendered_templates_cache.insert(
                                pnpm_workspace_path.to_string_lossy(),
                                RenderedTemplate {
                                    path: pnpm_workspace_path.clone(),
                                    content: yaml_to_string(&pnpm_workspace)?,
                                    context: None,
                                },
                            );

                            log_ok!(stdout, "Removed from pnpm-workspace");
                        }
                    }
                    Runtime::Bun => {
                        let package_json_path = modules_path.join("package.json");
                        if package_json_path.exists() {
                            let pkg_template = rendered_templates_cache.get(&package_json_path)?;
                            let mut package_json: ApplicationPackageJson =
                                if let Some(template) = pkg_template {
                                    json_from_str(&template.content)
                                        .context("Failed to parse package.json")?
                                } else {
                                    json_from_str(&std::fs::read_to_string(&package_json_path)?)
                                        .context("Failed to parse package.json")?
                                };

                            if let Some(ref mut workspaces) = package_json.workspaces {
                                if let Some(pos) = workspaces.iter().position(|p| p == project_name)
                                {
                                    workspaces.remove(pos);
                                }
                            }

                            rendered_templates_cache.insert(
                                package_json_path.to_string_lossy(),
                                RenderedTemplate {
                                    path: package_json_path.clone(),
                                    content: json_to_string_pretty(&package_json)?,
                                    context: None,
                                },
                            );

                            log_ok!(stdout, "Removed from package.json workspaces");
                        }
                    }
                }
            }
            ArtifactType::ClientSdk => {
                if project_type == ProjectType::Service || project_type == ProjectType::Worker {
                    remove_project_from_client_sdk(
                        rendered_templates_cache,
                        modules_path,
                        &manifest_data.app_name,
                        project_name,
                    )?;

                    log_ok!(stdout, "Removed from universal SDK");
                }
            }
            ArtifactType::ModulesTsconfig => {
                writeln!(
                    stdout,
                    "[INFO] Modules tsconfig.json cleanup not yet implemented"
                )?;
            }
        }
    }

    Ok(())
}
