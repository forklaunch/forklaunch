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
    /// Backend hosting type. Optional — defaults to "ecs-fargate" on the platform
    /// when absent. Allowed: "ecs-fargate" | "ecs-ec2".
    #[serde(rename = "hostingType", alias = "hosting_type", default, skip_serializing_if = "Option::is_none")]
    pub(crate) hosting_type: Option<String>,
    /// Run the component's container in privileged mode. Required for nsjail's
    /// namespace clone in the deploy sandbox; only valid with hostingType
    /// "ecs-ec2" (Fargate forbids privileged mode). Optional — absent means false.
    #[serde(
        rename = "privileged",
        alias = "privileged",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) privileged: Option<bool>,
}

/// A port this project serves traffic on, recorded from what the running
/// process actually bound during `openapi export`.
///
/// Deployment used to infer this from environment variable NAMES — anything
/// ending `_PORT`, minus a denylist of dependency ports — which provisioned a
/// load balancer for a scaffolded `WS_PORT` no code read, and a container port
/// mapping for a Redis port the service only connects to. Recording what was
/// bound removes the guess, and carries the real value: a websocket server on
/// 12000 is written as 12000, not the conventional 11000.
#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ServingPort {
    pub(crate) port: u16,
    /// "http" or "ws". Drives the target group's protocol.
    pub(crate) protocol: String,
    /// A path on THIS port answering 2xx to a plain GET. The load balancer
    /// health-checks the port it forwards to, so every serving port needs one.
    pub(crate) health_path: String,
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
    /// Absent on manifests written before this existed, which is what keeps
    /// the old inference alive as a fallback instead of forcing a flag day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ports: Option<Vec<ServingPort>>,
}

/// Compliance configuration stored in the `[compliance]` section of `manifest.toml`.
/// All fields are optional so existing manifests parse without changes.
///
/// Note: entity field classifications and retention policies are NOT stored here.
/// They are scanned from source code at release/audit time and uploaded directly
/// to the platform. This prevents leaking sensitive compliance metadata in the repo.
#[derive(Debug, Serialize, Deserialize, Content, Clone, Default)]
pub(crate) struct ComplianceManifestConfig {
    /// Allowed deployment regions (e.g., `["us-east-1", "eu-west-1"]`).
    /// Enforced by the platform compiler at deploy time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) data_residency: Vec<String>,

    /// Required secrets that must be present as environment variables at boot.
    /// The framework's SecretsAccessor validates these at startup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) secrets: Vec<String>,
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
            #[serde(default, skip_serializing_if = "Option::is_none")]
            $vis compliance: Option<crate::core::manifest::ComplianceManifestConfig>,
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
                        compliance: shadow.compliance.clone(),

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

/// Record the ports each project reported during `openapi export`.
///
/// Returns true when anything changed, so the caller only rewrites the
/// manifest when there is something new to persist. A project missing from
/// `reported` is left untouched rather than cleared: a service that failed to
/// export, or one built with a framework that does not yet report, must not
/// silently lose a declaration it already had.
pub(crate) fn apply_serving_ports(
    projects: &mut [ProjectEntry],
    reported: &std::collections::HashMap<String, Vec<ServingPort>>,
) -> bool {
    let mut changed = false;

    for project in projects.iter_mut() {
        let Some(ports) = reported.get(&project.name) else {
            continue;
        };

        let mut sorted = ports.clone();
        sorted.sort_by_key(|p| p.port);

        let differs = match &project.ports {
            Some(existing) => {
                existing.len() != sorted.len()
                    || existing.iter().zip(sorted.iter()).any(|(a, b)| {
                        a.port != b.port
                            || a.protocol != b.protocol
                            || a.health_path != b.health_path
                    })
            }
            None => true,
        };

        if differs {
            project.ports = Some(sorted);
            changed = true;
        }
    }

    changed
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
        // Filled in by `openapi export`, from what the running process bound.
        ports: None,
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

#[cfg(test)]
mod serving_port_tests {
    use super::*;
    use std::collections::HashMap;

    fn project(name: &str, ports: Option<Vec<ServingPort>>) -> ProjectEntry {
        ProjectEntry {
            r#type: ProjectType::Service,
            name: name.to_string(),
            description: String::new(),
            variant: None,
            resources: None,
            routers: None,
            metadata: None,
            ports,
        }
    }

    fn port(port: u16, protocol: &str) -> ServingPort {
        ServingPort {
            port,
            protocol: protocol.to_string(),
            health_path: "/health".to_string(),
        }
    }

    #[test]
    fn records_reported_ports_in_port_order() {
        let mut projects = vec![project("vault", None)];
        let mut reported = HashMap::new();
        reported.insert(
            "vault".to_string(),
            vec![port(11000, "ws"), port(8000, "http")],
        );

        assert!(apply_serving_ports(&mut projects, &reported));

        let recorded = projects[0].ports.as_ref().unwrap();
        assert_eq!(recorded[0].port, 8000);
        assert_eq!(recorded[1].port, 11000);
    }

    #[test]
    fn reports_no_change_when_ports_already_match() {
        // The caller only rewrites manifest.toml when something changed, so a
        // re-export of an unchanged app must not dirty the file.
        let mut projects = vec![project("vault", Some(vec![port(8000, "http")]))];
        let mut reported = HashMap::new();
        reported.insert("vault".to_string(), vec![port(8000, "http")]);

        assert!(!apply_serving_ports(&mut projects, &reported));
    }

    #[test]
    fn leaves_a_project_that_reported_nothing_untouched() {
        // A service that failed to export, or one built with a framework that
        // does not report yet, must not lose a declaration it already had.
        let mut projects = vec![
            project("vault", Some(vec![port(8000, "http")])),
            project("iam", None),
        ];
        let reported = HashMap::new();

        assert!(!apply_serving_ports(&mut projects, &reported));
        assert_eq!(projects[0].ports.as_ref().unwrap()[0].port, 8000);
        assert!(projects[1].ports.is_none());
    }

    #[test]
    fn notices_a_port_that_moved() {
        // WS_PORT is not fixed at 11000; a service that rebinds must be
        // re-recorded rather than kept at the old value.
        let mut projects = vec![project("vault", Some(vec![port(11000, "ws")]))];
        let mut reported = HashMap::new();
        reported.insert("vault".to_string(), vec![port(12000, "ws")]);

        assert!(apply_serving_ports(&mut projects, &reported));
        assert_eq!(projects[0].ports.as_ref().unwrap()[0].port, 12000);
    }

    #[test]
    fn omits_ports_from_a_manifest_that_has_none() {
        // skip_serializing_if keeps old manifests byte-identical, so adding
        // the field cannot churn every project's TOML.
        let rendered = to_string_pretty(&project("vault", None)).unwrap();
        assert!(!rendered.contains("ports"));
    }
}
