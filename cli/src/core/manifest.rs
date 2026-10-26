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
pub(crate) mod language;
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProjectLanguage {
    TypeScript,
    Rust,
}

#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ResourceInventory {
    pub(crate) database: Option<String>,
    pub(crate) cache: Option<String>,
    pub(crate) queue: Option<String>,
    pub(crate) object_store: Option<String>,
    pub(crate) redis_partition: Option<u32>,
    /// Platform-held capabilities this project uses through the managed
    /// gateway (payments, email, sms, whatsapp, voice, models), added with
    /// `forklaunch infra add`. Each becomes a release-manifest resource of the
    /// matching platform type, bound to this service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) capabilities: Option<Vec<String>>,
}

/// Top-level keys earlier CLIs wrote into `.forklaunch/manifest.toml` by
/// mistake: `init service|worker` serialized the project's derived template
/// data (`snake_case_name`) alongside the application's keys. Nothing reads
/// them; every manifest write drops them.
pub(crate) const RETIRED_MANIFEST_KEYS: &[&str] = &["snake_case_name"];

/// Whether `path` is an application manifest.
pub(crate) fn is_manifest_path(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "manifest.toml")
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| dir == ".forklaunch")
}

/// `text` without the retired top-level keys, every other byte kept. Text
/// that does not parse, or has none of them, is returned unchanged.
pub(crate) fn strip_retired_manifest_keys(text: &str) -> String {
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return text.to_string();
    };
    if !RETIRED_MANIFEST_KEYS.iter().any(|key| doc.contains_key(key)) {
        return text.to_string();
    }
    // The first key carries the header comment as its decor; hand it on.
    let leading = doc
        .iter()
        .next()
        .map(|(key, _)| key.to_string())
        .filter(|key| RETIRED_MANIFEST_KEYS.contains(&key.as_str()))
        .and_then(|key| doc.key(&key).and_then(|k| k.leaf_decor().prefix()).and_then(|p| p.as_str()).map(str::to_owned));
    for key in RETIRED_MANIFEST_KEYS {
        doc.remove(key);
    }
    if let Some(prefix) = leading {
        if let Some((mut key, _)) = doc.iter_mut().next() {
            key.leaf_decor_mut().set_prefix(prefix);
        }
    }
    doc.to_string()
}

/// `rendered` (a manifest a command wrote from its own model) laid out like
/// `original`, the file on disk: unchanged content keeps the original bytes;
/// otherwise every table keeps the original's key order (new keys after) and
/// the original's leading comment. The TypeScript commands write the manifest
/// from a struct in field order and the Rust ones from a sorted table, so
/// without this each rewrote the whole file into its own order.
pub(crate) fn preserve_manifest_layout(original: &str, rendered: &str) -> String {
    use toml_edit::{DocumentMut, Item, Table};
    let (Ok(before), Ok(after)) = (
        toml::from_str::<toml::Value>(original),
        toml::from_str::<toml::Value>(rendered),
    ) else {
        return rendered.to_string();
    };
    if before == after {
        return original.to_string();
    }
    let (Ok(source), Ok(mut doc)) = (original.parse::<DocumentMut>(), rendered.parse::<DocumentMut>()) else {
        return rendered.to_string();
    };
    fn order(table: &mut Table, source: &Table) {
        let rank: HashMap<String, usize> =
            source.iter().enumerate().map(|(i, (key, _))| (key.to_string(), i)).collect();
        table.sort_values_by(|a, _, b, _| {
            rank.get(a.get()).unwrap_or(&usize::MAX).cmp(rank.get(b.get()).unwrap_or(&usize::MAX))
        });
        for (key, item) in table.iter_mut() {
            match (item, source.get(key.get())) {
                (Item::Table(inner), Some(Item::Table(source))) => order(inner, source),
                (Item::ArrayOfTables(tables), Some(Item::ArrayOfTables(sources))) => {
                    for (i, inner) in tables.iter_mut().enumerate() {
                        // Projects are matched by name, other entries by place.
                        let name = inner.get("name").and_then(Item::as_str).map(str::to_owned);
                        let matched = name
                            .and_then(|name| {
                                sources.iter().find(|t| t.get("name").and_then(Item::as_str) == Some(name.as_str()))
                            })
                            .or_else(|| sources.get(i));
                        if let Some(source) = matched {
                            order(inner, source);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn renumber(table: &mut Table, position: &mut isize) {
        for (_, item) in table.iter_mut() {
            match item {
                Item::Table(inner) => {
                    inner.set_position(Some(*position));
                    *position += 1;
                    renumber(inner, position);
                }
                Item::ArrayOfTables(tables) => {
                    for inner in tables.iter_mut() {
                        inner.set_position(Some(*position));
                        *position += 1;
                        renumber(inner, position);
                    }
                }
                _ => {}
            }
        }
    }
    // The leading comment is the first key's decor: it stays at the top as
    // the original had it, whichever key comes first after sorting.
    fn first_prefix(table: &Table) -> Option<String> {
        let (key, _) = table.iter().next()?;
        table.key(key)?.leaf_decor().prefix()?.as_str().map(str::to_owned)
    }
    fn set_first_prefix(table: &mut Table, prefix: &str) {
        if let Some((mut key, _)) = table.iter_mut().next() {
            key.leaf_decor_mut().set_prefix(prefix);
        }
    }
    let header = first_prefix(source.as_table()).unwrap_or_default();
    set_first_prefix(doc.as_table_mut(), "");
    order(doc.as_table_mut(), source.as_table());
    set_first_prefix(doc.as_table_mut(), &header);
    let mut position = 1;
    renumber(doc.as_table_mut(), &mut position);
    doc.to_string()
}

/// The `toml::Value` form of [`strip_retired_manifest_keys`].
pub(crate) fn remove_retired_manifest_keys(value: &mut toml::Value) {
    if let Some(table) = value.as_table_mut() {
        for key in RETIRED_MANIFEST_KEYS {
            table.remove(*key);
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Content, Clone, Default)]
pub(crate) struct ProjectMetadata {
    /// Explicit generated service SDK ownership; absent retains legacy layouts.
    #[serde(rename="sdkLayout", alias="sdk_layout", default, skip_serializing_if="Option::is_none")]
    pub(crate) sdk_layout: Option<String>,
    /// Explicit opt-out for components without a framework HTTP contract exporter.
    /// Absent means enabled, preserving existing service and worker behavior.
    #[serde(
        rename = "contractExport",
        alias = "contract_export",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) contract_export: Option<bool>,
    /// TypeScript service whose SDKs `forklaunch sdk build` compiles from its
    /// contract modules (see `sdk::typescript_build`). Typed so manifest
    /// rewrites (for example `change application`) keep it.
    #[serde(
        rename = "contractBuild",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) contract_build: Option<bool>,
    #[serde(
        rename = "staticPath",
        alias = "static_path",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) static_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    pub(crate) r#type: Option<String>,
    #[serde(
        rename = "crateName",
        alias = "crate_name",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) crate_name: Option<String>,
    #[serde(
        rename = "cratePath",
        alias = "crate_path",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) crate_path: Option<String>,
    #[serde(
        rename = "contractArtifactPath",
        alias = "contract_artifact_path",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) contract_artifact_path: Option<String>,
    #[serde(
        rename = "openapiPath",
        alias = "openapi_path",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) openapi_path: Option<String>,
    #[serde(
        rename = "asyncapiPath",
        alias = "asyncapi_path",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) asyncapi_path: Option<String>,
    #[serde(
        rename = "sdkTargets",
        alias = "sdk_targets",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(crate) sdk_targets: Vec<String>,
    #[serde(
        rename = "workerType",
        alias = "worker_type",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) worker_type: Option<String>,
    #[serde(rename="workerRuntime",default,skip_serializing_if="Option::is_none")]
    pub(crate) worker_runtime: Option<String>,
    /// Worker project owns an independently runnable producer HTTP/MCP service.
    #[serde(rename="workerService",default,skip_serializing_if="Option::is_none")]
    pub(crate) worker_service: Option<bool>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub(crate) persistence: Option<String>,
    /// Generated durable worker supports a separate control-only executable role.
    #[serde(
        rename = "workerControlService",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) worker_control_service: Option<bool>,
    /// Backend hosting type. Optional — defaults to "ecs-fargate" on the platform
    /// when absent. Allowed: "ecs-fargate" | "ecs-ec2".
    #[serde(
        rename = "hostingType",
        alias = "hosting_type",
        default,
        skip_serializing_if = "Option::is_none"
    )]
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

/// A port a project serves beyond its main HTTP port, declared as
/// `[[projects.serves]]` in `manifest.toml`.
///
/// Names the ENV VAR rather than the number because the value is
/// environment-specific (a service can bind 8002 in compose and 8000 in
/// production); the platform resolves `port_env` against the target
/// environment at deploy time. HTTP (`PORT`) and MCP are framework conventions
/// and are never declared here — only ports the project actually constructs a
/// server for, such as a websocket server on `WS_PORT`.
#[derive(Debug, Serialize, Deserialize, Content, Clone)]
pub(crate) struct ServingPort {
    /// "ws" | "http" | "mcp"
    pub(crate) protocol: String,
    #[serde(rename = "port_env", alias = "portEnv")]
    pub(crate) port_env: String,
    /// Must answer 2xx to a plain GET; the platform defaults it to `/health`.
    #[serde(
        rename = "health_path",
        alias = "healthPath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) health_path: Option<String>,
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
    /// Extra serving ports, forwarded verbatim into the release manifest.
    /// Absent means nothing beyond HTTP (and MCP) is fronted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) serves: Option<Vec<ServingPort>>,
}

/// Compliance configuration stored in the `[compliance]` section of `manifest.toml`.
/// All fields are optional so existing manifests parse without changes.
///
/// Note: entity field classifications and retention policies are NOT stored here.
/// They are scanned from source code at release/audit time and uploaded directly
/// to the platform. This prevents leaking sensitive compliance metadata in the repo.
/// `[framework]`: which Rust framework and blueprint crates the application uses.
/// Kept on typed manifest views so TypeScript lifecycle edits preserve it.
#[derive(Debug, Serialize, Deserialize, Content, Clone, Default, PartialEq)]
pub(crate) struct FrameworkDependencies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) blueprint_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
}

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
            // Shared by application and project views so TypeScript lifecycle
            // edits preserve mixed-language workspace and artifact metadata.
            #[serde(skip_serializing_if = "Option::is_none")]
            $vis language: Option<String>,
            #[serde(
                rename = "applicationLanguage",
                alias = "application_language",
                skip_serializing_if = "Option::is_none"
            )]
            $vis application_language: Option<String>,
            #[serde(
                default,
                rename = "sdkTargets",
                alias = "sdk_targets",
                skip_serializing_if = "Vec::is_empty"
            )]
            $vis sdk_targets: Vec<String>,
            #[serde(
                rename = "cargoWorkspace",
                alias = "cargo_workspace",
                skip_serializing_if = "Option::is_none"
            )]
            $vis cargo_workspace: Option<String>,
            #[serde(
                rename = "contractArtifactPath",
                alias = "contract_artifact_path",
                skip_serializing_if = "Option::is_none"
            )]
            $vis contract_artifact_path: Option<String>,
            #[serde(
                rename = "openapiPath",
                alias = "openapi_path",
                skip_serializing_if = "Option::is_none"
            )]
            $vis openapi_path: Option<String>,
            #[serde(
                rename = "asyncapiPath",
                alias = "asyncapi_path",
                skip_serializing_if = "Option::is_none"
            )]
            $vis asyncapi_path: Option<String>,
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
            // Rust framework crate selection; see `core::forklaunch_state`.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            $vis framework: Option<crate::core::manifest::FrameworkDependencies>,
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
                        language: shadow.language.clone(),
                        application_language: shadow.application_language.clone(),
                        sdk_targets: shadow.sdk_targets.clone(),
                        cargo_workspace: shadow.cargo_workspace.clone(),
                        contract_artifact_path: shadow.contract_artifact_path.clone(),
                        openapi_path: shadow.openapi_path.clone(),
                        asyncapi_path: shadow.asyncapi_path.clone(),
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
                        framework: shadow.framework.clone(),

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
        serves: None,
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
mod retired_key_tests {
    use super::*;

    const HEADER: &str = "# Generated by ForkLaunch\n# File: manifest.toml\n\n";

    #[test]
    fn stray_snake_case_name_is_dropped_and_the_header_kept() {
        let stray = format!("{HEADER}snake_case_name = \"accounts\"\nsdkTargets = [\"typescript\"]\napp_name = \"zebra\"\n\n[[projects]]\nname = \"accounts\"\n");
        let clean = format!("{HEADER}sdkTargets = [\"typescript\"]\napp_name = \"zebra\"\n\n[[projects]]\nname = \"accounts\"\n");
        assert_eq!(strip_retired_manifest_keys(&stray), clean);
        assert_eq!(strip_retired_manifest_keys(&clean), clean);
        // A project's own key of that name is not top-level and stays.
        let nested = "app_name = \"zebra\"\n\n[[projects]]\nname = \"a\"\nsnake_case_name = \"a\"\n";
        assert_eq!(strip_retired_manifest_keys(nested), nested);
    }

    #[test]
    fn manifest_writes_keep_the_files_key_and_table_order() {
        let original = "app_name = \"zebra\"\nid = \"x\"\n\n[[projects]]\nname = \"a\"\ntype = \"Service\"\n\n[projects.resources]\ndatabase = \"postgresql\"\n\n[[projects]]\nname = \"b\"\ntype = \"Library\"\n\n[project_peer_topology]\nzebra = [\"a\"]\n";
        // Same content in struct order: the original bytes come back.
        let reordered = "# Generated by ForkLaunch\n\nid = \"x\"\napp_name = \"zebra\"\n\n[project_peer_topology]\nzebra = [\"a\"]\n\n[[projects]]\ntype = \"Service\"\nname = \"a\"\n\n[projects.resources]\ndatabase = \"postgresql\"\n\n[[projects]]\ntype = \"Library\"\nname = \"b\"\n";
        assert_eq!(preserve_manifest_layout(original, reordered), original);
        // A change keeps the order, and a new key follows the existing ones.
        let changed = reordered.replace("id = \"x\"\n", "id = \"x\"\nlicense = \"MIT\"\n").replace("\"Library\"", "\"Worker\"");
        assert_eq!(
            preserve_manifest_layout(original, &changed),
            original.replace("id = \"x\"\n", "id = \"x\"\nlicense = \"MIT\"\n").replace("\"Library\"", "\"Worker\"")
        );
    }

    #[test]
    fn service_and_worker_data_do_not_serialize_derived_names() {
        let manifest = "id='x'\ncli_version='1.0.0'\napp_name='demo'\nmodules_path='src'\nruntime='node'\nformatter='prettier'\nlinter='oxlint'\nvalidator='zod'\nhttp_framework='express'\ntest_framework='vitest'\napp_description='d'\nauthor='a'\nlicense='MIT'\nsnake_case_name='old'\nprojects=[]\nproject_peer_topology={}\n";
        let service: ServiceManifestData = toml::from_str(manifest).unwrap();
        assert!(!to_string_pretty(&service).unwrap().contains("snake_case_name"));
        let worker: WorkerManifestData = toml::from_str(manifest).unwrap();
        assert!(!to_string_pretty(&worker).unwrap().contains("snake_case_name"));
    }
}
