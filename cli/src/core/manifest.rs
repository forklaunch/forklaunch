use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use library::LibraryManifestData;
use ramhorns::Content;
use serde::{Deserialize, Serialize};
use service::ServiceManifestData;
use toml::to_string_pretty;
use worker::WorkerManifestData;

use super::rendered_template::RenderedTemplate;
use crate::{
    constants::{
        Database, ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST,
        ERROR_FAILED_TO_ADD_ROUTER_METADATA_TO_MANIFEST, ERROR_FAILED_TO_CREATE_MANIFEST,
        ERROR_FAILED_TO_REMOVE_PROJECT_METADATA_FROM_MANIFEST, Infrastructure, WorkerType,
    },
    core::manifest::{application::ApplicationManifestData, router::RouterManifestData},
};

pub(crate) mod application;
pub(crate) mod library;
pub(crate) mod router;
pub(crate) mod service;
pub(crate) mod worker;

crate::mutable_enum! {
    #[allow(dead_code)]
    #[derive(Debug)]
    pub(crate) enum ManifestData<'a> {
        Application(ApplicationManifestData),
        Service(ServiceManifestData),
        Library(LibraryManifestData),
        Router(RouterManifestData),
        Worker(WorkerManifestData),
    }
}

pub(crate) trait ManifestConfig {
    fn app_name(&self) -> &String;
    fn formatter(&self) -> &String;
    fn linter(&self) -> &String;
    fn test_framework(&self) -> &Option<String>;
    fn projects(&self) -> &Vec<ProjectEntry>;
    fn projects_mut(&mut self) -> &mut Vec<ProjectEntry>;
    fn project_peer_topology_mut(&mut self) -> &mut HashMap<String, Vec<String>>;
}

pub(crate) trait ProjectManifestConfig {
    fn name(&self) -> &String;
    fn description(&self) -> &String;
}

#[derive(Debug)]

pub(crate) struct ApplicationInitializationMetadata {
    pub(crate) app_name: String,
    pub(crate) database: Option<String>,
}

#[derive(Debug)]

pub(crate) struct ProjectInitializationMetadata {
    pub(crate) project_name: String,
    pub(crate) database: Option<Database>,
    pub(crate) infrastructure: Option<Vec<Infrastructure>>,
    pub(crate) description: Option<String>,
    pub(crate) worker_type: Option<WorkerType>,
}

#[derive(Debug)]
pub(crate) struct RouterInitializationMetadata {
    pub(crate) project_name: String,
    pub(crate) router_name: Option<String>,
}

pub(crate) enum InitializableManifestConfigMetadata {
    #[allow(dead_code)]
    Application(ApplicationInitializationMetadata),
    Project(ProjectInitializationMetadata),
    Router(RouterInitializationMetadata),
}
pub(crate) trait InitializableManifestConfig {
    fn initialize(&self, metadata: InitializableManifestConfigMetadata) -> Self;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) enum ProjectType {
    Service,
    Library,
    Worker,
}

impl Content for ProjectType {}

#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ResourceInventory {
    pub(crate) database: Option<String>,
    pub(crate) cache: Option<String>,
    pub(crate) queue: Option<String>,
    pub(crate) object_store: Option<String>,
    pub(crate) redis_partition: Option<u32>,
}

pub(crate) fn next_available_redis_partition(projects: &[ProjectEntry]) -> u32 {
    let used: std::collections::HashSet<u32> = projects
        .iter()
        .filter_map(|p| p.resources.as_ref())
        .filter_map(|r| r.redis_partition)
        .collect();
    let mut partition = 0u32;
    while used.contains(&partition) {
        partition += 1;
    }
    partition
}

#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ProjectMetadata {
    pub(crate) r#type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ProjectEntry {
    pub(crate) r#type: ProjectType,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) variant: Option<String>,
    pub(crate) resources: Option<ResourceInventory>,
    pub(crate) routers: Option<Vec<String>>,
    pub(crate) metadata: Option<ProjectMetadata>,
}

#[macro_export]
macro_rules! internal_config_struct {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident: $ty:ty
            ),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(
                #[serde(default)]
                $(#[$field_meta])*
                $field_vis $field: $ty
            ),*,
            $vis id: String,
            $vis cli_version: String,
            $vis app_name: String,
            $vis modules_path: String,
            $vis docker_compose_path: Option<String>,
            $vis dockerfile: Option<String>,
            $vis git_repository: Option<String>,
            #[serde(skip_serializing, skip_deserializing)]
            $vis camel_case_app_name: String,
            #[serde(skip_serializing, skip_deserializing)]
            $vis pascal_case_app_name: String,
            #[serde(skip_serializing, skip_deserializing)]
            $vis kebab_case_app_name: String,
            #[serde(skip_serializing, skip_deserializing)]
            $vis title_case_app_name: String,
            $vis app_description: String,
            $vis linter: String,
            $vis formatter: String,
            $vis validator: String,
            $vis http_framework: String,
            $vis runtime: String,
            $vis test_framework: Option<String>,
            $vis projects: Vec<crate::core::manifest::ProjectEntry>,
            $vis project_peer_topology: std::collections::HashMap<String, Vec<String>>,
            $vis author: String,
            $vis license: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis platform_application_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis platform_organization_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis release_version: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis release_git_commit: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis release_git_branch: Option<String>,
        }
    };
}

#[macro_export]
macro_rules! config_struct {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident: $ty:ty
            ),*
            $(,)?
        }
    ) => {
        crate::internal_config_struct! {
            $(#[$meta])*
            $vis struct $name {
                $(
                    $(#[$field_meta])*
                    $field_vis $field: $ty
                ),*,

                #[serde(skip_serializing)]
                $vis is_eslint: bool,

                #[serde(skip_serializing)]
                $vis is_biome: bool,

                #[serde(skip_serializing)]
                $vis is_oxlint: bool,

                #[serde(skip_serializing)]
                $vis is_prettier: bool,

                #[serde(skip_serializing)]
                $vis is_express: bool,

                #[serde(skip_serializing)]
                $vis is_hyper_express: bool,

                #[serde(skip_serializing)]
                $vis is_zod: bool,

                #[serde(skip_serializing)]
                $vis is_typebox: bool,

                #[serde(skip_serializing)]
                $vis is_bun: bool,

                #[serde(skip_serializing)]
                $vis is_node: bool,

                #[serde(skip_serializing)]
                $vis is_vitest: bool,

                #[serde(skip_serializing)]
                $vis is_jest: bool,
            }
        }

        paste::paste! {
            crate::internal_config_struct! {
                $(#[$meta])*
                #[derive(Deserialize)]
                struct [<Shadow $name>] {
                    $(
                        $(#[$field_meta])*
                        $field_vis $field: $ty
                    ),*
                }
            }

            impl From<[<Shadow $name>]> for $name {
                fn from(shadow: [<Shadow $name>]) -> Self {
                    Self {
                        id: shadow.id.clone(),
                        cli_version: shadow.cli_version.clone(),
                        app_name: shadow.app_name.clone(),
                        modules_path: shadow.modules_path.clone(),
                        docker_compose_path: shadow.docker_compose_path.clone(),
                        dockerfile: shadow.dockerfile.clone(),
                        git_repository: shadow.git_repository.clone(),
                        camel_case_app_name: shadow.camel_case_app_name.clone(),
                        pascal_case_app_name: shadow.pascal_case_app_name.clone(),
                        kebab_case_app_name: shadow.kebab_case_app_name.clone(),
                        title_case_app_name: shadow.title_case_app_name.clone(),
                        app_description: shadow.app_description.clone(),
                        linter: shadow.linter.clone(),
                        formatter: shadow.formatter.clone(),
                        validator: shadow.validator.clone(),
                        http_framework: shadow.http_framework.clone(),
                        runtime: shadow.runtime.clone(),
                        test_framework: shadow.test_framework.clone(),
                        projects: shadow.projects.clone(),
                        project_peer_topology: shadow.project_peer_topology.clone(),
                        author: shadow.author.clone(),
                        license: shadow.license.clone(),
                        platform_application_id: shadow.platform_application_id.clone(),
                        platform_organization_id: shadow.platform_organization_id.clone(),
                        release_version: shadow.release_version.clone(),
                        release_git_commit: shadow.release_git_commit.clone(),
                        release_git_branch: shadow.release_git_branch.clone(),

                        is_eslint: shadow.linter == "eslint",
                        is_biome: shadow.formatter == "biome",
                        is_oxlint: shadow.linter == "oxlint",
                        is_prettier: shadow.formatter == "prettier",
                        is_express: shadow.http_framework == "express",
                        is_hyper_express: shadow.http_framework == "hyper-express",
                        is_zod: shadow.validator == "zod",
                        is_typebox: shadow.validator == "typebox",
                        is_bun: shadow.runtime == "bun",
                        is_node: shadow.runtime == "node",
                        is_vitest: if let Some(test_framework) = &shadow.test_framework { test_framework == "vitest" } else { false },
                        is_jest: if let Some(test_framework) = &shadow.test_framework { test_framework == "jest" } else { false },
                        $(
                            $field: shadow.$field
                        ),*
                    }
                }
            }

            impl<'de> Deserialize<'de> for $name {
                fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    let shadow: [<Shadow $name>] = Deserialize::deserialize(deserializer)?;
                    Ok(shadow.into())
                }
            }
        }

        impl crate::core::manifest::ManifestConfig for $name {
            fn app_name(&self) -> &String {
                &self.app_name
            }
            fn formatter(&self) -> &String {
                &self.formatter
            }
            fn linter(&self) -> &String {
                &self.linter
            }
            fn test_framework(&self) -> &Option<String> {
                &self.test_framework
            }
            fn projects(&self) -> &Vec<crate::core::manifest::ProjectEntry> {
                &self.projects
            }
            fn projects_mut(&mut self) -> &mut Vec<crate::core::manifest::ProjectEntry> {
                &mut self.projects
            }
            fn project_peer_topology_mut(&mut self) -> &mut std::collections::HashMap<String, Vec<String>> {
                &mut self.project_peer_topology
            }
        }
    };
}

pub(crate) fn generate_manifest(
    path_dir: &String,
    data: &ApplicationManifestData,
) -> Result<Option<RenderedTemplate>> {
    let config_str = to_string_pretty(&data).with_context(|| ERROR_FAILED_TO_CREATE_MANIFEST)?;
    let manifest_path = Path::new(path_dir)
        .join(".forklaunch")
        .join("manifest.toml");

    if manifest_path.exists() {
        return Ok(None);
    }

    Ok(Some(RenderedTemplate {
        path: manifest_path,
        content: config_str,
        context: None,
    }))
}

pub(crate) fn add_project_definition_to_manifest<
    T: ManifestConfig + ProjectManifestConfig + InitializableManifestConfig + Serialize,
>(
    r#type: ProjectType,
    manifest_data: &mut T,
    variant: Option<String>,
    resources: Option<ResourceInventory>,
    routers: Option<Vec<String>>,
    metadata: Option<ProjectMetadata>,
) -> Result<String> {
    let name = manifest_data.name().to_owned();
    let description = manifest_data.description().to_owned();
    for project in manifest_data.projects().iter() {
        if project.name == name {
            return Ok(to_string_pretty(&manifest_data)
                .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST)?);
        }
    }

    manifest_data.projects_mut().push(ProjectEntry {
        r#type,
        name: name.clone(),
        description,
        variant,
        resources,
        routers,
        metadata,
    });

    let app_name = manifest_data.app_name().to_owned();
    manifest_data
        .project_peer_topology_mut()
        .entry(app_name)
        .or_insert_with(Vec::new)
        .push(name.clone());

    Ok(to_string_pretty(&manifest_data)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST)?)
}

pub(crate) fn add_router_definition_to_manifest(
    manifest_data: &mut RouterManifestData,
    serivce_name: &String,
) -> Result<(ProjectType, String)> {
    let name = manifest_data.router_name.clone();
    for project in manifest_data.projects().iter() {
        if let Some(routers) = &project.routers {
            for router in routers.iter() {
                if router == &name {
                    return Ok((
                        project.r#type.clone(),
                        to_string_pretty(&manifest_data)
                            .with_context(|| ERROR_FAILED_TO_ADD_ROUTER_METADATA_TO_MANIFEST)?,
                    ));
                }
            }
        }
    }

    let project = manifest_data
        .projects_mut()
        .iter_mut()
        .find(|project| &project.name == serivce_name)
        .unwrap();

    if project.routers == None {
        project.routers = Some(vec![])
    }

    project.routers.as_mut().unwrap().push(name);

    Ok((
        project.r#type.clone(),
        to_string_pretty(&manifest_data)
            .with_context(|| ERROR_FAILED_TO_ADD_ROUTER_METADATA_TO_MANIFEST)?,
    ))
}

pub(crate) fn remove_project_definition_from_manifest(
    manifest_data: &mut ApplicationManifestData,
    project_name: &String,
) -> Result<String> {
    let project = manifest_data
        .projects_mut()
        .iter_mut()
        .position(|project| &project.name == project_name)
        .unwrap();

    manifest_data.projects_mut().remove(project);

    manifest_data
        .project_peer_topology
        .iter_mut()
        .for_each(|(_, values)| {
            if values.contains(&project_name) {
                values.remove(values.iter().position(|x| x == project_name).unwrap());
            }
        });

    Ok(to_string_pretty(&manifest_data)
        .with_context(|| ERROR_FAILED_TO_REMOVE_PROJECT_METADATA_FROM_MANIFEST)?)
}

pub(crate) fn remove_router_definition_from_manifest(
    manifest_data: &mut RouterManifestData,
    project_name: &String,
    router_name: &String,
) -> Result<String> {
    manifest_data.projects.iter_mut().for_each(|project| {
        if &project.name == project_name {
            let routers = project.routers.clone().unwrap();
            project.routers.as_mut().unwrap().remove(
                routers
                    .iter()
                    .position(|router| router == router_name)
                    .unwrap(),
            );
        }
    });

    Ok(to_string_pretty(&manifest_data)
        .with_context(|| ERROR_FAILED_TO_REMOVE_PROJECT_METADATA_FROM_MANIFEST)?)
}
