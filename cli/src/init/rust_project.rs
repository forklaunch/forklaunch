use std::{
    collections::HashSet,
    fs::{create_dir_all, read_to_string, remove_dir_all, remove_file, rename, write},
    io::{IsTerminal, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use convert_case::{Case, Casing};
use include_dir::Dir;
use indexmap::IndexMap;
use ramhorns::Content;
use serde_json::{Value as JsonValue, json};
use serde_yml::{from_str as yaml_from_str, to_string as yaml_to_string};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use toml::{Value, map::Map};
use uuid::Uuid;

use crate::core::{
    artifact_paths::{self, Artifact},
    docker::{
        Command as DockerCommand, DockerBuild, DockerCompose, DockerNetwork, DockerService,
        HealthTest, Healthcheck, Restart,
    },
    name::validate_name,
    rendered_template::{RenderedTemplate, write_rendered_templates},
    template::render_embedded_template,
};

mod conversion;
mod kind_change;
mod library_rename;
mod discovery;
pub(crate) mod dependencies;
mod mixed;
mod agent;
mod auth_env;
mod language_images;
mod local_runtime;
mod typescript_image;
mod modules;
mod native_layout;
mod native_mappers;
mod contracts_layout;
mod service_database;
mod sdk_consumer;

pub(crate) fn ensure_service_sdk_deletable(root: &Path, service_path: &str) -> Result<()> {
    sdk_consumer::ensure_service_deletable(root, service_path)
}
pub(crate) fn reconcile_rust_client_aggregate(root: &Path) -> Result<()> {
    sdk_consumer::reconcile_local_aggregate(root)
}
pub(crate) fn rename_service_sdk_dependency(root: &Path, old_path: &str, new_path: &str, old: &str, new: &str) -> Result<()> {
    sdk_consumer::rename_service(root, old_path, new_path, old, new)
}

mod sdk_typescript;
mod worker;
pub(crate) use modules::generate_rust_module;


/// Formats generated Rust with the application's rustfmt settings.
pub(crate) fn format_rust_source(path: &Path, source: &str) -> String {
    native_layout::format_source(path, source)
}

pub(crate) fn ensure_native_infrastructure_sources(root: &Path) -> Result<()> {
    ensure_embedded_framework(root, false)
}

pub(crate) const TYPESCRIPT_SDK_TSCONFIG: &str =
    include_str!("../templates/rust/application/typescript-sdk-tsconfig.json");

pub(crate) const PROJECT_LANGUAGE_VARIANTS: [&str; 3] = ["typescript", "rust", "mixed"];
pub(crate) const SDK_TARGET_VARIANTS: [&str; 4] = ["typescript", "ts", "rust", "all"];

pub(crate) fn selected_language(matches: &ArgMatches) -> &str {
    matches
        .get_one::<String>("language")
        .map(String::as_str)
        .unwrap_or("typescript")
}

pub(crate) fn uses_rust_generation(matches: &ArgMatches) -> bool {
    matches!(selected_language(matches), "rust" | "mixed")
}

pub(super) fn sdk_targets_from_matches(matches: &ArgMatches) -> Vec<String> {
    matches
        .get_many::<String>("sdk-targets")
        .map(|targets| normalize_sdk_targets(targets.map(String::as_str)))
        .unwrap_or_else(default_sdk_targets)
}

fn default_sdk_targets() -> Vec<String> {
    vec!["typescript".to_string(), "rust".to_string()]
}

fn normalize_sdk_targets<'a>(targets: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut include_typescript = false;
    let mut include_rust = false;

    for target in targets {
        match target {
            "typescript" | "ts" => include_typescript = true,
            "rust" => include_rust = true,
            "all" => {
                include_typescript = true;
                include_rust = true;
            }
            _ => {}
        }
    }

    let mut normalized = Vec::new();
    if include_typescript {
        normalized.push("typescript".to_string());
    }
    if include_rust {
        normalized.push("rust".to_string());
    }

    if normalized.is_empty() {
        default_sdk_targets()
    } else {
        normalized
    }
}

pub(crate) use language_images::summary as rust_image_summary;
pub(crate) use typescript_image::summary as typescript_image_summary;
pub(crate) use typescript_image::planned_processes as typescript_planned_processes;

pub(crate) fn adopt_typescript_image(root: &Path) -> Result<()> {
    typescript_image::adopt_image(root)
}

/// The generated TypeScript image (see `typescript_image::managed_image`).
/// Host framework workspaces a `[framework] path` application builds from;
/// empty for published crates.
pub(crate) fn framework_mounts(root: &Path) -> Vec<PathBuf> {
    local_runtime::framework_contexts(root)
        .map(|(_, contexts)| contexts.into_iter().map(|(_, path)| path).collect())
        .unwrap_or_default()
}

pub(crate) fn managed_typescript_image(root: &Path) -> Option<(PathBuf, bool)> {
    typescript_image::managed_image(root)
}

pub(crate) fn preflight_mixed_images(root: &Path) -> Result<()> {
    language_images::preflight(root)
}

pub(crate) fn preflight_typescript_runtime_change(root: &Path) -> Result<()> {
    typescript_image::preflight_runtime(root)
}

pub(super) fn prepare_mixed_workspace_images(root: &Path) -> Result<()> {
    mixed::ensure_workspace(root, false)?;
    language_images::reconcile(root)
}

/// Records `metadata.language = "typescript"` for a TypeScript project of a
/// mixed application. Prebuilt TypeScript modules other than IAM never pass
/// through the contract build that records it, and a project without a
/// language reads as native to the mixed-runtime wiring.
pub(crate) fn record_typescript_language(root: &Path, name: &str) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let source = read_to_string(&path)?;
    let mut document: toml_edit::DocumentMut = source.parse()?;
    if document.get("language").and_then(|item| item.as_str()) != Some("mixed") {
        return Ok(());
    }
    let Some(projects) = document.get_mut("projects").and_then(|item| item.as_array_of_tables_mut()) else {
        return Ok(());
    };
    let Some(project) = projects.iter_mut().find(|project| project.get("name").and_then(|item| item.as_str()) == Some(name)) else {
        return Ok(());
    };
    let metadata = project
        .entry("metadata")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("Project metadata must be a table")?;
    if metadata.contains_key("language") {
        return Ok(());
    }
    metadata.insert("language", toml_edit::value("typescript"));
    write(&path, document.to_string())?;
    Ok(())
}

pub(crate) fn refresh_mixed_images(root: &Path) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    if path.is_file() && (read_manifest_value(&path)?.get("language").and_then(Value::as_str) == Some("mixed") || crate::sdk::typescript_build::any_contract_builds(root)?) {
        language_images::preflight(root)?;
        language_images::reconcile(root)?;
    } else if path.is_file() && typescript_image::managed_image(root).is_some_and(|(_, unchanged)| unchanged) {
        // A pure TypeScript application's generated image lists every
        // package and process; adding or removing one must refresh it, as
        // the mixed path above does. An edited image is the user's.
        typescript_image::reconcile(root)?;
    }
    // Every TypeScript project command ends here; CI follows language changes.
    crate::core::github_configs::reconcile_github_configs(root)
}

fn sdk_targets_from_manifest(manifest: &Value) -> Vec<String> {
    manifest
        .get("sdkTargets")
        .and_then(Value::as_array)
        .map(|targets| normalize_sdk_targets(targets.iter().filter_map(Value::as_str)))
        .unwrap_or_else(default_sdk_targets)
}

fn sdk_targets_toml_array(sdk_targets: &[String]) -> String {
    sdk_targets
        .iter()
        .map(|target| format!("\"{}\"", escape_toml_string(target)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn has_typescript_sdk_target(sdk_targets: &[String]) -> bool {
    sdk_targets.iter().any(|target| target == "typescript")
}

pub(crate) fn generate_rust_application(matches: &ArgMatches) -> Result<()> {
    let name = required_name(matches, "application")?;
    let modules: Vec<_> = matches
        .try_get_many::<String>("modules")
        .ok()
        .flatten()
        .into_iter()
        .flatten()
        .collect();
    if let Some(module) = modules
        .iter()
        .find(|module| modules::TYPESCRIPT_ONLY_MODULES.contains(&module.as_str()))
    {
        bail!("{module} is a TypeScript module; add it with `forklaunch init module --module {module}`. Rust services call it through its generated Rust SDK");
    }
    if modules.iter().any(|module| modules::BuiltinModule::from_variant(module).is_none()) {
        bail!("Rust application module generation supports iam-base, cac-base, ecommerce-stripe, billing-base and billing-stripe");
    }
    if !modules.is_empty()
        && option(matches, "database").is_some_and(|database| database != "postgresql")
    {
        bail!("Rust modules require PostgreSQL");
    }
    let language = selected_language(matches).to_string();
    let dryrun = matches.get_flag("dryrun");
    let root_path = matches
        .get_one::<String>("path")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&name));
    let description =
        option(matches, "description").unwrap_or_else(|| format!("{name} application"));
    let author = option(matches, "author").unwrap_or_default();
    let license = option(matches, "license").unwrap_or_else(|| "MIT".to_string());
    let sdk_targets = sdk_targets_from_matches(matches);
    let manifest = application_manifest(
        &name,
        &description,
        &author,
        &license,
        &language,
        &sdk_targets,
    );

    let mut templates = vec![
        RenderedTemplate {
            path: root_path.join("Cargo.toml"),
            content: root_cargo_toml(&license, &author),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("rust-toolchain.toml"),
            content: render_static_rust_template("rust/application/rust-toolchain.toml"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("rustfmt.toml"),
            content: render_static_rust_template("rust/application/rustfmt.toml"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("clippy.toml"),
            content: render_static_rust_template("rust/application/clippy.toml"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join(".gitignore"),
            content: render_static_rust_template("rust/application/gitignore"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("README.md"),
            content: render_rust_template(
                "rust/application/README.md",
                &RustReadmeTemplate {
                    name: name.clone(),
                    description: description.clone(),
                },
            ),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join(".dockerignore"),
            content: render_static_rust_template("rust/application/dockerignore"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("Dockerfile"),
            content: render_static_rust_template("rust/application/Dockerfile"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("docker-compose.yaml"),
            content: rust_docker_compose_yaml(&name)?,
            context: None,
        },
        RenderedTemplate {
            path: root_path.join(".forklaunch").join("manifest.toml"),
            content: manifest,
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("config").join("local.toml"),
            content: render_static_rust_template("rust/application/config/local.toml"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("config").join("test.toml"),
            content: render_static_rust_template("rust/application/config/test.toml"),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join(".env.example"),
            content: render_static_rust_template("rust/application/env.example"),
            context: None,
        },
    ];

    if has_typescript_sdk_target(&sdk_targets) {
        templates.extend(
            mixed_typescript_sdk_templates(&root_path, &name, &sdk_targets)
                .into_iter()
                .map(|mut template| {
                    let relative = template
                        .path
                        .strip_prefix(&root_path)
                        .expect("SDK template path");
                    let relative = relative
                        .to_string_lossy()
                        .replace("sdks/typescript", "src/client-sdk/typescript")
                        .replace("typescript/src/index.ts", "typescript/index.ts");
                    template.path = root_path.join(relative);
                    template.content = template
                        .content
                        .replace("sdks/typescript", "src/client-sdk/typescript")
                        .replace("./src/index.ts", "./index.ts")
                        .replace("src/**/*.ts", "**/*.ts");
                    template
                }),
        );
    }

    let mut module_names = std::collections::HashSet::new();
    for module in &modules {
        let kind =
            modules::BuiltinModule::from_variant(module).context("Unsupported Rust module")?;
        if !module_names.insert(kind.service_name()) {
            bail!(
                "Conflicting Rust modules target the same service: {}",
                kind.service_name()
            );
        }
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    write_rust_rendered_templates(&templates, dryrun, &mut stdout)?;
    if !dryrun {
        // Select framework crates before anything else resolves Rust sources.
        dependencies::configure(&root_path, matches)?;
    }
    ensure_embedded_framework(&root_path, dryrun)?;
    for module in modules {
        let kind =
            modules::BuiltinModule::from_variant(module).context("Unsupported Rust module")?;
        modules::generate_module_at(&root_path, kind.service_name(), matches, dryrun, kind)?;
    }
    if !dryrun {
        crate::core::github_configs::reconcile_github_configs(&root_path)?;
    }
    log_ok!(
        stdout,
        "Generated {} application at {}",
        language,
        root_path.display()
    );
    writeln!(stdout, "{}", super::application::CONTEXT_HINT)?;

    Ok(())
}

pub(crate) fn generate_rust_service(matches: &ArgMatches) -> Result<()> {
    generate_rust_project(matches, RustProjectKind::Service)
}

pub(crate) fn generate_rust_agent(matches: &ArgMatches) -> Result<()> {
    let name = matches.get_one::<String>("name").context("Agent name is required")?.clone();
    let dryrun = matches.get_flag("dryrun");
    let servers = agent::mcp_servers(matches)?;
    let app_root = find_manifest_root(
        &matches
            .get_one::<String>("base_path")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir().context("Failed to read current directory")?),
    )?;
    let services = agent::services(matches, &app_root)?;
    generate_rust_project(matches, RustProjectKind::Service)?;
    if dryrun {
        return Ok(());
    }
    agent::mark_variant(&app_root, &name)?;
    agent::compose(&app_root, &name, &services, &servers)?;
    agent::sources(
        &app_root,
        &name,
        matches.get_one::<String>("instructions").map(String::as_str),
        &services,
        &servers,
    )?;
    // Fill local values for the agent's new required settings.
    language_images::reconcile(&app_root)
}

pub(crate) fn generate_rust_worker(matches: &ArgMatches) -> Result<()> {
    generate_rust_project(matches, RustProjectKind::Worker)
}

pub(crate) fn generate_rust_library(matches: &ArgMatches) -> Result<()> {
    if let Some(services)=matches.try_get_many::<String>("sdk-services").ok().flatten() { return sdk_consumer::generate(matches,services.cloned().collect()); }
    generate_rust_project(matches, RustProjectKind::Library)
}

pub(crate) fn generate_rust_router(matches: &ArgMatches) -> Result<()> {
    let dryrun = matches.get_flag("dryrun");
    let router_name = required_name(matches, "router")?;
    let router_module = router_name.to_case(Case::Snake);
    let service_path = matches
        .get_one::<String>("base_path")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().context("Failed to read current directory")?);
    let app_root = find_manifest_root(&service_path)?;
    let service_name = service_path
        .file_name()
        .and_then(|part| part.to_str())
        .context("Rust router path must point at a service crate directory")?
        .to_string();
    if !is_rust_project(&app_root, &service_name)?
        || detect_rust_project_kind(&app_root, &service_name)? != RustProjectKind::Service
    {
        bail!("Rust routers require the path of a registered Rust service crate");
    }
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let registered_path = crate_path_for_project(&manifest, &service_name)
        .context("Rust service has no registered crate path")?;
    if app_root.join(registered_path).canonicalize()? != service_path.canonicalize()? {
        bail!("Rust router path must match the registered service crate directory");
    }
    if contracts_layout::is_native_service(&service_path) {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        native_layout::generate_router(&service_path, &router_name, dryrun, &mut stdout)?;
        return add_router_to_manifest(&app_root, &service_name, &router_name, dryrun);
    }
    let builtin = modules::module_kind(&app_root, &service_name)?.is_some();
    if builtin {
        let flat = service_path.join("lib.rs").exists();
        let shared = shared_module_contracts(&service_path);
        let registry = service_path.join("api/contracts/registry.rs");
        let lib = read_to_string(if shared { registry } else { service_path.join(if flat { "sdk.rs" } else { "src/lib.rs" }) })?;
        let (main, hook) = if shared {
            (read_to_string(service_path.join("api/routes/mod.rs"))?, "extensions::register_router(")
        } else {
            (read_to_string(service_path.join(if flat { "server.rs" } else { "src/main.rs" }))?, "routes::register_router(")
        };
        if !lib.contains("routes::contracts()") || !main.contains(hook) {
            bail!(
                "This existing module does not expose custom router hooks. Add routes::contracts() to its registry and routes::register_router(...) around its runtime router before generating extensions"
            );
        }
    }
    if !builtin && !service_path.join("src/contracts/mod.rs").exists() {
        bail!(
            "This service predates build-time contracts; add a shared src/contracts module before generating a typed router"
        );
    }
    let routes_dir = service_routes_directory(&service_path);
    let route_file = routes_dir.join(format!("{router_module}.rs"));
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    write_text_file_preserving_existing(
        &route_file,
        &if builtin && shared_module_contracts(&service_path) {
            let source = native_router_rs(&router_name);
            let (contract, handler) = source.split_once("pub fn register(").context("module route template")?;
            write_text_file_preserving_existing(&service_path.join("api/contracts/extensions").join(format!("{router_module}.rs")),
                // Declarations compile in the contract host, which has no
                // runtime dependencies; handler imports stay with the handler.
                &contract.replace("use forklaunch_axum::ForkLaunchRouter;", "").replace("use axum::Json;\n", ""), dryrun, &mut stdout)?;
            format!("use axum::Json;\nuse forklaunch_axum::ForkLaunchRouter;\npub use crate::api::contracts::extensions::{router_module}::{{contract, contracts, ROUTE_NAME}};\npub fn register({handler}")
        } else if builtin {
            native_router_rs(&router_name)
        } else {
            router_rs(&router_name)
        },
        dryrun,
        &mut stdout,
    )?;

    if !builtin && service_path.join("src/contracts/mod.rs").exists() {
        let contracts_dir = service_path.join("src/contracts");
        write_text_file_preserving_existing(
            &contracts_dir.join(format!("{router_module}.rs")),
            &router_contract_rs(&router_name),
            dryrun,
            &mut stdout,
        )?;
        let mut modules = rust_route_modules(&contracts_dir)?;
        modules.retain(|module| module != "models");
        modules.push(router_module.clone());
        write_text_file(
            &contracts_dir.join("mod.rs"),
            &contracts_mod_rs(&modules),
            dryrun,
            &mut stdout,
        )?;
    }
    rewrite_routes_mod(&routes_dir, dryrun, &mut stdout)?;
    add_router_to_manifest(&app_root, &service_name, &router_name, dryrun)?;

    log_ok!(
        stdout,
        "Generated Rust router {} in {}",
        router_name,
        service_path.display()
    );

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RustProjectKind {
    Service,
    Worker,
    Library,
}

#[derive(Content)]
struct EmptyRustTemplate {}

#[derive(Content)]
struct RustRootCargoTemplate {
    license: String,
    authors: String,
}

#[derive(Content)]
struct RustReadmeTemplate {
    name: String,
    description: String,
}

#[derive(Content)]
struct RustApplicationManifestTemplate {
    id: String,
    cli_version: String,
    name: String,
    description: String,
    author: String,
    license: String,
    language: String,
    sdk_targets: String,
}

#[derive(Content)]
struct RustCrateTemplate {
    name: String,
    description: String,
    crate_module: String,
    has_binary: bool,
    uses_framework: bool,
    framework_rs_path: String,
}

#[derive(Content)]
struct RustRouterTemplate {
    router_module: String,
    router_name: String,
    route_title: String,
    response_name: String,
}

#[derive(Content)]
struct RustRoutesModTemplate {
    modules: Vec<RustRouteModuleTemplate>,
    has_health: bool,
}

#[derive(Content)]
struct RustRouteModuleTemplate {
    module: String,
}

#[derive(Clone)]
struct RustWorkspaceProject {
    name: String,
    kind: RustProjectKind,
    crate_path: String,
}

impl RustProjectKind {
    pub(crate) fn manifest_type(self) -> &'static str {
        match self {
            Self::Service => "Service",
            Self::Worker => "Worker",
            Self::Library => "Library",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Worker => "worker",
            Self::Library => "library",
        }
    }
}

/// Required settings of native processes that `.env` does not provide.
pub(crate) fn native_env_gaps(app_root: &Path) -> Result<Vec<(String, String)>> {
    local_runtime::env_gaps(app_root)
}

/// Fill the generated local values native processes need (as `init` does);
/// returns what still needs a person.
pub(crate) fn sync_native_env(app_root: &Path) -> Result<Vec<(String, String)>> {
    if !is_rust_app_root(app_root)? || !app_root.join("Cargo.toml").is_file() {
        return Ok(vec![]);
    }
    language_images::reconcile(app_root)?;
    local_runtime::env_gaps(app_root)
}

pub(crate) fn is_rust_app_root(app_root: &Path) -> Result<bool> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    if !manifest_path.exists() {
        return Ok(false);
    }
    let manifest = read_manifest_value(&manifest_path)?;
    Ok(manifest
        .get("language")
        .and_then(Value::as_str)
        .map(|language| language == "rust" || language == "mixed")
        .unwrap_or(false)
        || manifest
            .get("runtime")
            .and_then(Value::as_str)
            .map(|runtime| runtime == "tokio")
            .unwrap_or(false))
}

pub(crate) fn is_rust_project(app_root: &Path, name: &str) -> Result<bool> {
    if !is_rust_app_root(app_root)? {
        return Ok(false);
    }
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let manifest = read_manifest_value(&manifest_path)?;
    if project_table(&manifest, name)
        .and_then(|project| project.get("variant"))
        .and_then(Value::as_str)
        == Some("storefront")
    {
        return Ok(false);
    }
    if let Some(language) = project_table(&manifest, name)
        .and_then(|project| project.get("metadata"))
        .and_then(|metadata| metadata.get("language"))
        .and_then(Value::as_str)
    {
        return Ok(language == "rust");
    }
    Ok(discovery::path(app_root, &manifest, name)?.is_some())
}

pub(crate) fn registered_project_names(
    app_root: &Path,
    kind: RustProjectKind,
) -> Result<HashSet<String>> {
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    Ok(manifest
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|project| project.get("type").and_then(Value::as_str) == Some(kind.manifest_type()))
        .filter_map(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect())
}

/// Earlier generated build scripts wrote declarations to the crate root. Move
/// only that exact generated output; a customized destination stays as written.
fn migrate_compliance_artifact(crate_dir: &Path) -> Result<()> {
    const RERUN: &str = "println!(\"cargo:rerun-if-changed=compliance.json\");";
    const JOIN: &str = ".join(\"compliance.json\")";
    const WRITE: &str = "std::fs::write(path, metadata)?;";
    let build = crate_dir.join("build.rs");
    let Ok(source) = read_to_string(&build) else {
        return Ok(());
    };
    if source.matches(RERUN).count() != 1
        || source.matches(JOIN).count() != 1
        || source.matches(WRITE).count() != 1
    {
        return Ok(());
    }
    let line = source
        .lines()
        .find(|line| line.contains(WRITE))
        .context("Expected compliance write")?;
    let indent = &line[..line.len() - line.trim_start().len()];
    let create = "std::fs::create_dir_all(path.parent().expect(\"crate-local artifact directory\"))?;";
    let statements = if line.trim() == WRITE {
        format!("{create}\n{indent}{WRITE}")
    } else {
        format!("{create} {WRITE}")
    };
    let updated = source
        .replace(RERUN, "println!(\"cargo:rerun-if-changed=.forklaunch/compliance.json\");")
        .replace(JOIN, ".join(\".forklaunch/compliance.json\")")
        .replace(WRITE, &statements);
    write(&build, native_layout::format_source(&build, &updated))?;
    // The crate-root file was build output; the next build regenerates it.
    let legacy = crate_dir.join("compliance.json");
    if legacy.is_file() {
        remove_file(legacy)?;
    }
    Ok(())
}

pub(crate) fn sync_rust_project(app_root: &Path, kind: RustProjectKind, name: &str) -> Result<()> {
    if !is_rust_project(app_root, name)? {
        bail!("Project {name} is not a Rust project; refusing native sync cleanup");
    }
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let crate_path = discovery::path(app_root, &manifest, name)?
        .context("Missing Rust project path")?;
    let crate_dir = app_root.join(&crate_path);
    if !crate_dir.join("Cargo.toml").exists() {
        delete_rust_project(app_root, kind, name)?;
        return Ok(());
    }

    migrate_compliance_artifact(&crate_dir)?;
    mixed::ensure_root_ignore_rules(app_root)?;
    add_cargo_member(app_root, &crate_path.to_string_lossy(), false)?;
    let description = cargo_description(&crate_dir.join("Cargo.toml"))?
        .unwrap_or_else(|| format!("Generated {name} {}", kind.label()));
    discovery::register(app_root, kind, name, &crate_path, &description)?;
    discovery::routers(app_root, name, &crate_dir)?;
    if let Some(host) = contracts_layout::directory(&crate_dir) {
        if host.join("Cargo.toml").is_file() {
            let host = host.canonicalize()?;
            add_cargo_member(app_root, &host.strip_prefix(app_root.canonicalize()?)?.to_string_lossy(), false)?;
        }
    }
    if crate_dir.join("sdk/rust/Cargo.toml").is_file() {
        add_cargo_member(app_root, &format!("{}/sdk/rust", crate_path.display()), false)?;
    }
    if let Some(module) = modules::module_kind(app_root, name)? {
        modules::ensure_module_compose(app_root, name, module, false)?;
        return ensure_rust_package_scripts(app_root, kind, name, false);
    }
    if kind == RustProjectKind::Worker
        && let Some(backend) = worker::backend(app_root, name)?
    {
        add_project_to_manifest(
            app_root,
            kind,
            name,
            &description,
            &ArgMatches::default(),
            false,
        )?;
        worker::mark(app_root, name, backend)?;
        worker::compose(app_root, name, backend)?;
        return ensure_rust_package_scripts(app_root, kind, name, false);
    }
    let flat = crate_dir.join("lib.rs").exists();
    let templates = if flat && kind == RustProjectKind::Service {
        let host = contracts_layout::directory(&crate_dir).context("Missing service contracts")?.canonicalize()?;
        add_cargo_member(app_root, &host.strip_prefix(app_root.canonicalize()?)?.to_string_lossy(), false)?;
        // As init does: the mapper module is opt-in (`--mappers`), so a sync
        // must not add the placeholder `domain/mappers/mod.rs` that nothing
        // declares. A project with mappers already has the file.
        native_mappers::templates(
            &crate_dir,
            false,
            native_layout::service_templates(&crate_dir, name, &description),
        )?
    } else {
        let templates = rust_crate_templates(&crate_dir, name, &description, kind);
        if flat {
            native_layout::flatten(&crate_dir, name, kind, templates)
        } else {
            templates
        }
    };
    let templates = native_layout::framework_paths(app_root, templates)
        .into_iter()
        .filter(|template| !template.path.exists())
        .collect::<Vec<_>>();
    write_rust_rendered_templates(
        &templates,
        false,
        &mut StandardStream::stdout(ColorChoice::Always),
    )?;
    add_project_to_manifest(
        app_root,
        kind,
        name,
        &description,
        &ArgMatches::default(),
        false,
    )?;
    ensure_rust_local_library_dependencies(app_root, false)?;
    ensure_rust_docker_compose_project(app_root, kind, name, false)?;
    ensure_rust_package_scripts(app_root, kind, name, false)
}

pub(crate) fn add_rust_workspace_member(app_root: &Path, member: &str) -> Result<()> {
    add_cargo_member(app_root, member, false)
}

pub(crate) fn sync_all_rust_projects(app_root: &Path, stdout: &mut StandardStream) -> Result<()> {
    // Reconcile removed projects before any surviving project rebuilds the shared image.
    cleanup_missing_rust_projects(app_root)?;
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let mut names = std::collections::BTreeSet::new();
    for project in manifest
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if project
            .get("metadata")
            .and_then(|metadata| metadata.get("language"))
            .and_then(Value::as_str)
            == Some("rust")
            && let Some(name) = project.get("name").and_then(Value::as_str)
        {
            names.insert(name.to_owned());
        }
    }
    for relative in discovery::roots(&manifest) {
        let directory = app_root.join(relative);
        if !directory.is_dir() { continue; }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if entry.path().join("Cargo.toml").is_file()
                && let Some(name) = entry.file_name().to_str()
                && is_rust_project(app_root, name)?
            {
                names.insert(name.to_owned());
            }
        }
    }
    for name in names {
        let kind = detect_rust_project_kind(app_root, &name)?;
        sync_rust_project(app_root, kind, &name)?;
        log_ok!(stdout, "Rust {} '{}' synced", kind.label(), name);
    }

    cleanup_missing_rust_projects(app_root)?;
    ensure_rust_local_library_dependencies(app_root, false)?;
    Ok(())
}

pub(crate) fn delete_rust_project(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
) -> Result<()> {
    delete_rust_project_inner(app_root, kind, name, true)?;
    warn_if_workspace_empty(app_root)
}

/// Cargo rejects a workspace without members, which the generated root
/// scripts (`build`, `test`, `lint`) run.
fn warn_if_workspace_empty(app_root: &Path) -> Result<()> {
    let empty = read_to_string(app_root.join("Cargo.toml"))
        .ok()
        .and_then(|source| source.parse::<toml_edit::DocumentMut>().ok())
        .and_then(|cargo| cargo.get("workspace")?.get("members")?.as_array().map(|members| members.is_empty()))
        .unwrap_or(false);
    if empty {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_warn!(
            stdout,
            "No Rust projects remain. Cargo rejects the empty workspace, so the root build, test and lint scripts fail until you add a Rust project or remove the Rust workspace files (Cargo.toml, Cargo.lock, Dockerfile.rust, rust-toolchain.toml, rustfmt.toml, clippy.toml) and the cargo steps in package.json."
        );
    }
    Ok(())
}

fn delete_rust_project_inner(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
    reconcile: bool,
) -> Result<()> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let crate_path = crate_path_for_project(&manifest, name)
        .unwrap_or_else(|| Path::new("crates").join(name).to_string_lossy().to_string());
    if contracts_layout::directory(&app_root.join(&crate_path)).is_some() {
        sdk_consumer::ensure_service_deletable(app_root, &crate_path)?;
    }
    let capabilities = project_table(&manifest, name)
        .and_then(|project| project.get("resources"))
        .and_then(|resources| resources.get("capabilities"))
        .and_then(Value::as_array)
        .map(|features| {
            features
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // Socket outputs are located from the project's metadata, which the manifest
    // no longer holds once the project is removed.
    let metadata = project_table(&manifest, name)
        .and_then(|project| project.get("metadata"))
        .and_then(Value::as_table)
        .cloned();
    language_images::preflight(app_root)?;
    remove_project_from_manifest_value(&mut manifest, name, Some(kind))?;
    write_manifest_value(&manifest_path, &manifest)?;
    crate::infra::capabilities::cleanup_deleted_native_service(app_root, name, &capabilities)?;
    remove_cargo_member(app_root, &crate_path)?;
    // A sync cleanup may run after the entire project directory was removed.
    // Its generated host remains an owned workspace member even without a Cargo manifest.
    remove_cargo_member(app_root, &format!("{crate_path}/.forklaunch/contracts"))?;
    if let Some(host) = contracts_layout::directory(&app_root.join(&crate_path)) {
        let host = host.canonicalize()?;
        remove_cargo_member(app_root, &host.strip_prefix(app_root.canonicalize()?)?.to_string_lossy())?;
        if host.parent() == app_root.canonicalize()?.join(&crate_path).parent() { remove_path_if_exists(&host)?; }
    }
    remove_cargo_member(app_root, &format!("{crate_path}/domain"))?;
    remove_cargo_member(app_root, &format!("{crate_path}/sdk/rust"))?;
    cleanup_generated_outputs_for_deleted_rust_project(app_root, kind, name, metadata.as_ref())?;
    remove_rust_docker_compose_project(app_root, name)?;
    crate::infra::native_local::release_deleted_service(app_root)?;
    local_runtime::release_unused_kafka(app_root)?;
    remove_rust_package_scripts(app_root, name)?;
    if kind == RustProjectKind::Library {
        remove_registered_library_dependency(app_root, &manifest, name, &crate_path)?;
    }
    if reconcile { ensure_rust_local_library_dependencies(app_root, false)?; }

    let crate_dir = app_root.join(&crate_path);
    if crate_dir.exists() {
        remove_dir_all(&crate_dir)
            .with_context(|| format!("Failed to remove {}", crate_dir.display()))?;
    }

    if reconcile {
        language_images::reconcile(app_root)?;
    }
    // Local values only the deleted project's processes read.
    local_runtime::prune_project_env(app_root, name, &[])?;
    Ok(())
}

fn cleanup_generated_outputs_for_deleted_rust_project(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
    metadata: Option<&Map<String, Value>>,
) -> Result<()> {
    if !matches!(kind, RustProjectKind::Service | RustProjectKind::Worker) {
        return Ok(());
    }

    if crate::sdk::websocket::cleanup_service(app_root, name, metadata)? {
        remove_cargo_member(
            app_root,
            &crate::sdk::sdk_base(app_root)?
                .join(format!("rust/{name}-websocket-sdk"))
                .to_string_lossy(),
        )?;
    }
    crate::core::openapi_export::export_registered_sockets(
        app_root,
        name,
        metadata,
        &json!({"sockets": {}}),
    )?;
    remove_rust_project_artifacts(app_root, name)?;

    let sdk_member = crate::sdk::sdk_base(app_root)?
        .join("rust")
        .join(format!("{name}-sdk"))
        .to_string_lossy()
        .to_string();
    remove_cargo_member(app_root, &sdk_member)?;
    remove_path_if_exists(&app_root.join(&sdk_member))?;
    crate::sdk::generate::cleanup_service_generated_files(app_root, name)?;
    Ok(())
}

pub(crate) fn remove_rust_project_artifacts(app_root: &Path, name: &str) -> Result<()> {
    for artifact_kind in ["contracts", "openapi", "asyncapi", "socket-contracts"] {
        remove_path_if_exists(&app_root.join(".forklaunch").join(artifact_kind).join(name))?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if path.is_dir() {
        remove_dir_all(path).with_context(|| format!("Failed to remove {}", path.display()))?;
    } else if path.exists() {
        remove_file(path).with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn change_rust_project(
    app_root: &Path,
    current_name: &str,
    new_name: Option<&str>,
    description: Option<&str>,
    database: Option<&str>,
    worker_type: Option<&str>,
    to_kind: Option<RustProjectKind>,
    dryrun: bool,
) -> Result<String> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let current_crate_path = crate_path_for_project(&manifest, current_name).unwrap_or_else(|| {
        Path::new("crates")
            .join(current_name)
            .to_string_lossy()
            .to_string()
    });
    let target_name = new_name.unwrap_or(current_name);
    let target_crate_path = if target_name == current_name {
        current_crate_path.clone()
    } else {
        Path::new(&current_crate_path)
            .parent()
            .unwrap_or_else(|| Path::new("crates"))
            .join(target_name)
            .to_string_lossy()
            .to_string()
    };
    let current_kind = project_table(&manifest, current_name)
        .and_then(|project| project.get("type"))
        .and_then(Value::as_str)
        .and_then(kind_from_manifest_type)
        .unwrap_or(RustProjectKind::Library);
    let target_kind = to_kind.unwrap_or(current_kind);
    if target_kind == RustProjectKind::Service && let Some(database) = database {
        if database != "postgresql" { bail!("Rust service storage supports PostgreSQL only"); }
        if modules::module_kind(app_root,current_name)?.is_none() && !app_root.join(&current_crate_path).join("persistence/database.rs").is_file() {
            bail!("Existing in-memory storage requires an explicit migration; initialize a new service with --database postgresql");
        }
    }
    let current_backend = if current_kind == RustProjectKind::Worker {
        worker::backend(app_root, current_name)?
    } else {
        None
    };
    let persistent_worker = current_backend.is_some();
    let current_project = project_table(&manifest, current_name);
    let target_database = database
        .or_else(|| {
            current_project
                .and_then(|project| project.get("resources"))
                .and_then(|resources| resources.get("database"))
                .and_then(Value::as_str)
        })
        .unwrap_or("postgresql");
    let target_worker_type = worker_type
        .or_else(|| {
            (current_kind == RustProjectKind::Worker)
                .then(|| {
                    current_project
                        .and_then(|project| project.get("metadata"))
                        .and_then(|metadata| metadata.get("workerType"))
                        .and_then(Value::as_str)
                })
                .flatten()
        })
        .unwrap_or("database");
    let target_backend = worker::Backend::from_options(target_worker_type, target_database);
    if target_kind == RustProjectKind::Worker && target_backend.is_none()
        && (current_kind != target_kind || worker_type.is_some() || database.is_some())
    {
        bail!("Unsupported Rust worker backend: {target_worker_type}/{target_database}; supported backends are PostgreSQL database, Redis, Kafka, and BullMQ");
    }
    let keep_persistent_worker = persistent_worker
        && target_kind == RustProjectKind::Worker
        && target_backend == current_backend;
    let convert_worker = target_kind == RustProjectKind::Worker
        && !keep_persistent_worker
        && (current_kind != target_kind || worker_type.is_some() || database.is_some());
    let persistent_module = modules::module_kind(app_root, current_name)?;
    let managed_capabilities = current_project
        .and_then(|project| project.get("resources"))
        .and_then(|resources| resources.get("capabilities"))
        .and_then(Value::as_array)
        .is_some_and(|features| !features.is_empty());
    let managed_infrastructure = current_kind == RustProjectKind::Service
        && current_project
            .and_then(|project| project.get("resources"))
            .is_some_and(|resources| {
                ["cache", "object_store"]
                    .iter()
                    .any(|key| resources.get(*key).and_then(Value::as_str).is_some())
            });
    if managed_capabilities && target_kind != RustProjectKind::Service {
        bail!(
            "Remove managed capabilities before converting this native service to another project kind"
        );
    }
    if managed_infrastructure && target_kind != RustProjectKind::Service {
        bail!(
            "Remove native infrastructure before converting this service to another project kind"
        );
    }
    if persistent_module.is_some()
        && (!matches!(target_kind, RustProjectKind::Service)
            || database.is_some_and(|database| database != "postgresql"))
    {
        bail!(
            "Rust built-in modules must remain services with their supported PostgreSQL configuration"
        );
    }

    let producer_worker = current_project
        .and_then(|project| project.get("metadata"))
        .is_some_and(|metadata| {
            metadata.get("workerService").and_then(Value::as_bool) == Some(true)
                && metadata.get("workerRuntime").and_then(Value::as_str) != Some("apalis")
        });
    if current_kind == RustProjectKind::Worker && target_kind == RustProjectKind::Worker && convert_worker && producer_worker
        && let (Some(from), Some(to)) = (current_backend, target_backend)
    {
        // Check the backend change against the current name first so a
        // refused change leaves the project untouched; then rename and
        // describe, then move the queue backend.
        if !dryrun {
            worker::change_producer_backend(app_root, current_name, from, to, true)?;
        }
        let name = if current_name != target_name || description.is_some() {
            change_rust_project(app_root, current_name, new_name, description, None, None, None, dryrun)?
        } else {
            current_name.to_owned()
        };
        let planned = worker::change_producer_backend(app_root, if dryrun { current_name } else { &name }, from, to, dryrun)?;
        if dryrun {
            let mut planned = planned;
            planned.extend([".forklaunch/manifest.toml".into(), "docker-compose.yaml".into(), ".env".into()]);
            crate::core::rendered_template::log_dryrun_plan(&mut StandardStream::stdout(ColorChoice::Always), &planned)?;
        }
        return Ok(name);
    }
    let native_service = current_kind == RustProjectKind::Service
        && persistent_module.is_none()
        && contracts_layout::is_native_service(&app_root.join(&current_crate_path));
    let kind_change = match (current_kind, target_kind) {
        (RustProjectKind::Service, RustProjectKind::Worker) if native_service => Some(Some(target_backend.context("Choose the worker type (-t database|redis|kafka)")?)),
        (RustProjectKind::Worker, RustProjectKind::Service) if producer_worker => Some(None),
        _ => None,
    };
    if let Some(backend) = kind_change {
        // Check the shape change against the current name first so a refused
        // change leaves the project untouched; then rename and describe, then
        // change the generated shape.
        if !dryrun {
            kind_change::change_kind(app_root, current_name, backend, true)?;
        }
        let name = if current_name != target_name || description.is_some() {
            change_rust_project(app_root, current_name, new_name, description, None, None, None, dryrun)?
        } else {
            current_name.to_owned()
        };
        let planned = kind_change::change_kind(app_root, if dryrun { current_name } else { &name }, backend, dryrun)?;
        if dryrun {
            crate::core::rendered_template::log_dryrun_plan(&mut StandardStream::stdout(ColorChoice::Always), &planned)?;
        }
        return Ok(name);
    }
    if current_kind != target_kind || convert_worker {
        conversion::preflight(app_root, current_name, current_kind)?;
    }
    if current_kind == RustProjectKind::Service && target_kind != current_kind {
        sdk_consumer::ensure_service_deletable(app_root, &current_crate_path)?;
    }

    if current_name != target_name {
        if project_table(&manifest, target_name).is_some()
            || app_root.join(&target_crate_path).exists()
            || app_root.join(&target_crate_path).with_file_name(format!("{target_name}-contracts")).exists()
        {
            bail!("Project destination {target_name} already exists");
        }
        for relative in [
            format!(".forklaunch/contracts/{target_name}"),
            format!(".forklaunch/openapi/{target_name}"),
            format!(".forklaunch/asyncapi/{target_name}"),
            crate::sdk::sdk_base(app_root)?
                .join(format!("rust/{target_name}-sdk"))
                .to_string_lossy()
                .into_owned(),
            format!(".forklaunch/socket-contracts/{target_name}"),
            format!(".forklaunch/socket-sdk-outputs/{target_name}"),
            crate::sdk::sdk_base(app_root)?
                .join(format!("rust/{target_name}-websocket-sdk"))
                .to_string_lossy()
                .into_owned(),
            crate::sdk::sdk_base(app_root)?
                .join(format!("typescript-websocket/{target_name}"))
                .to_string_lossy()
                .into_owned(),
        ] {
            if app_root.join(&relative).exists() {
                bail!("Generated output destination {relative} already exists");
            }
        }
    }
    if dryrun {
        let mut planned = vec![".forklaunch/manifest.toml".to_string()];
        if current_name != target_name {
            planned.insert(0, format!("{target_crate_path} (moved from {current_crate_path})"));
            planned.push("Cargo.toml (workspace members)".into());
        }
        if description.is_some() {
            planned.push(format!("{target_crate_path}/Cargo.toml"));
        }
        if current_name != target_name || current_kind != target_kind || worker_type.is_some() || database.is_some() {
            planned.extend([
                format!("{target_crate_path} sources"),
                manifest.get("docker_compose_path").and_then(Value::as_str).unwrap_or("docker-compose.yaml").to_string(),
                ".env".into(),
                "package.json (scripts)".into(),
                "generated SDK and contract outputs".into(),
            ]);
        }
        crate::core::rendered_template::log_dryrun_plan(&mut StandardStream::stdout(ColorChoice::Always), &planned)?;
        return Ok(target_name.to_string());
    }

    language_images::preflight(app_root)?;
    let library_edits = if current_kind == RustProjectKind::Library && current_name != target_name {
        library_rename::plan(app_root, &current_crate_path, &target_crate_path, current_name, target_name)?
    } else { Vec::new() };
    if current_name != target_name {
        let from = app_root.join(&current_crate_path);
        let to = app_root.join(&target_crate_path);
        if from.exists() {
            rename(&from, &to).with_context(|| {
                format!("Failed to rename {} to {}", from.display(), to.display())
            })?;
        }
        replace_cargo_member(app_root, &current_crate_path, &target_crate_path)?;
        if to.join(".forklaunch/contracts/Cargo.toml").is_file() {
            replace_cargo_member(app_root, &format!("{current_crate_path}/.forklaunch/contracts"), &format!("{target_crate_path}/.forklaunch/contracts"))?;
        }
        update_package_name(&to.join("Cargo.toml"), target_name)?;
        sdk_consumer::rename_service(app_root,&current_crate_path,&target_crate_path,current_name,target_name)?;
        let old_host = from.with_file_name(format!("{current_name}-contracts"));
        if old_host.join("Cargo.toml").is_file() {
            let new_host = to.with_file_name(format!("{target_name}-contracts"));
            anyhow::ensure!(!new_host.exists(), "Contract package rename destination exists");
            rename(&old_host, &new_host)?;
            replace_cargo_member(app_root, &old_host.strip_prefix(app_root)?.to_string_lossy(), &new_host.strip_prefix(app_root)?.to_string_lossy())?;
            native_layout::rename_project(&new_host, current_name, target_name)?;
        }
        if to.join("lib.rs").exists() {
            native_layout::rename_project(&to, current_name, target_name)?;
            if to.join("domain/Cargo.toml").exists() {
                replace_cargo_member(
                    app_root,
                    &format!("{current_crate_path}/domain"),
                    &format!("{target_crate_path}/domain"),
                )?;
            }
        }
    }

    {
        let project = project_table_mut(&mut manifest, current_name)
            .with_context(|| format!("Could not find Rust project {current_name} in manifest"))?;
        project.insert(
            "type".to_string(),
            Value::String(target_kind.manifest_type().to_string()),
        );
        project.insert("name".to_string(), Value::String(target_name.to_string()));
        if let Some(description) = description {
            project.insert(
                "description".to_string(),
                Value::String(description.to_string()),
            );
        }
        if let Some(database) = database {
            let resources = project
                .entry("resources")
                .or_insert_with(|| Value::Table(Map::new()))
                .as_table_mut()
                .context("Project resources must be a TOML table")?;
            resources.insert("database".to_string(), Value::String(database.to_string()));
        }
        let metadata = project
            .entry("metadata")
            .or_insert_with(|| Value::Table(Map::new()))
            .as_table_mut()
            .context("Project metadata must be a TOML table")?;
        metadata.insert("language".to_string(), Value::String("rust".to_string()));
        metadata.insert(
            "crateName".to_string(),
            Value::String(target_name.to_string()),
        );
        metadata.insert(
            "cratePath".to_string(),
            Value::String(target_crate_path.clone()),
        );
        // Artifacts inside the moved crate follow it, explicit legacy locations
        // follow the renamed project, and other custom locations stay put.
        let current_prefix = format!("{}/", current_crate_path.trim_end_matches('/'));
        for key in [
            Artifact::Contract.key(),
            Artifact::OpenApi.key(),
            Artifact::AsyncApi.key(),
            Artifact::SocketContract.key(),
            "asyncapiArtifactPath",
        ] {
            if let Some(relative) = metadata
                .get(key)
                .and_then(Value::as_str)
                .and_then(|path| path.strip_prefix(&current_prefix))
                .filter(|_| current_crate_path != target_crate_path)
            {
                let moved = format!("{}/{relative}", target_crate_path.trim_end_matches('/'));
                metadata.insert(key.into(), Value::String(moved));
            }
        }
        for artifact in [Artifact::Contract, Artifact::OpenApi, Artifact::AsyncApi] {
            if metadata.get(artifact.key()).and_then(Value::as_str)
                == Some(artifact_paths::legacy(current_name, artifact).as_str())
            {
                metadata.insert(
                    artifact.key().into(),
                    Value::String(artifact_paths::legacy(target_name, artifact)),
                );
            }
        }
        if contracts_layout::is_native_service(&app_root.join(&target_crate_path))
            || contracts_layout::has_local_sdk(&app_root.join(&target_crate_path)) {
            contracts_layout::set_artifact_paths(metadata, target_name);
        }
        if metadata
            .get("socketContractArtifactPath")
            .and_then(Value::as_str)
            == Some(
                format!(".forklaunch/socket-contracts/{current_name}/socket-contract.json")
                    .as_str(),
            )
        {
            metadata.insert(
                "socketContractArtifactPath".into(),
                Value::String(format!(
                    ".forklaunch/socket-contracts/{target_name}/socket-contract.json"
                )),
            );
        }
        if metadata.get("asyncapiArtifactPath").and_then(Value::as_str)
            == Some(format!(".forklaunch/asyncapi/{current_name}/asyncapi.json").as_str())
        {
            metadata.insert(
                "asyncapiArtifactPath".into(),
                Value::String(format!(".forklaunch/asyncapi/{target_name}/asyncapi.json")),
            );
        }
        if let Some(worker_type) = worker_type {
            metadata.insert(
                "workerType".to_string(),
                Value::String(worker_type.to_string()),
            );
        }
    }

    if persistent_worker && !keep_persistent_worker {
        project_table_mut(&mut manifest, target_name)
            .context("Expected worker")?
            .remove("variant");
    }
    write_manifest_value(&manifest_path, &manifest)?;
    if current_name != target_name && current_kind == target_kind {
        // Processes keep their settings, ports and infrastructure; only names move.
        local_runtime::rename_compose_project(app_root, current_name, target_name)?;
    }

    for (path, source) in library_edits {
        write(&path, source).with_context(|| format!("Failed to update library consumer {}", path.display()))?;
    }

    let crate_dir = app_root.join(&target_crate_path);
    if let Some(description) = description {
        let cargo_path = crate_dir.join("Cargo.toml");
        let mut cargo: toml_edit::DocumentMut = read_to_string(&cargo_path)?.parse()?;
        cargo["package"]["description"] = toml_edit::value(description);
        write(cargo_path, cargo.to_string())?;
    }
    if keep_persistent_worker {
        if current_name != target_name {
            worker::rename_sources(app_root, current_name, target_name)?;
            rename_rust_project_outputs(app_root, current_name, target_name)?;
            remove_rust_package_scripts(app_root, current_name)?;
        }
        worker::compose(
            app_root,
            target_name,
            current_backend.context("Expected worker backend")?,
        )?;
        return ensure_rust_package_scripts(app_root, target_kind, target_name, false)
            .map(|()| target_name.to_owned());
    }
    if let Some(module) = persistent_module {
        if current_name != target_name {
            modules::rename_sources(app_root, current_name, target_name)?;
            rename_rust_project_outputs(app_root, current_name, target_name)?;
        }
        ensure_rust_package_scripts(app_root, target_kind, target_name, false)?;
        modules::ensure_module_compose(app_root, target_name, module, false)?;
        if current_name != target_name {
            remove_rust_package_scripts(app_root, current_name)?;
        }
        return Ok(target_name.to_owned());
    }
    if current_kind == target_kind {
        // Metadata edits preserve application-owned source for every project kind.
        // Backend changes are applied explicitly below.
        if current_name != target_name {
            if !crate_dir.join("lib.rs").exists() {
                modules::rename_sources(app_root, current_name, target_name)?;
                if current_kind == RustProjectKind::Service {
                    rename_native_service_defaults(&crate_dir, current_name, target_name)?;
                }
            }
        }
    } else {
        match target_kind {
            RustProjectKind::Service => ensure_service_shape(&crate_dir, target_name)?,
            RustProjectKind::Worker => ensure_worker_shape(&crate_dir, target_name)?,
            RustProjectKind::Library => ensure_library_shape(&crate_dir, target_name)?,
        }
    }
    if current_kind != target_kind {
        remove_rust_docker_compose_project(app_root, current_name)?;
    }
    if current_name != target_name || current_kind != target_kind {
        remove_rust_package_scripts(app_root, current_name)?;
    }
    ensure_rust_docker_compose_project(app_root, target_kind, target_name, false)?;
    ensure_rust_package_scripts(app_root, target_kind, target_name, false)?;
    ensure_rust_local_library_dependencies(app_root, false)?;
    if convert_worker && let Some(backend) = target_backend {
        worker::convert(app_root, target_name, backend)?;
    }
    if current_name != target_name {
        rename_rust_project_outputs(app_root, current_name, target_name)?;
    }
    if current_kind != target_kind {
        if current_kind == RustProjectKind::Service {
            remove_cargo_member(app_root, &format!("{target_crate_path}/domain"))?;
            remove_path_if_exists(&crate_dir.join("domain/Cargo.toml"))?;
        }
        cleanup_generated_outputs_for_deleted_rust_project(
            app_root,
            current_kind,
            target_name,
            artifact_paths::application_metadata(app_root, target_name).as_ref(),
        )?;
    }

    language_images::reconcile(app_root)?;
    Ok(target_name.to_string())
}

// Generated artifacts retain their schemas while their service metadata and SDK
// names follow the renamed project. Other services' artifacts are left intact.
fn rename_rust_project_outputs(root: &Path, old: &str, new: &str) -> Result<()> {
    let local_artifacts = sdk_consumer::rename_local_artifacts(root, old, new)?;
    let imported = crate::sdk::websocket::imported_service(root, old)?;
    crate::infra::capabilities::rename_native_capabilities(root, old, new)?;
    for (kind, filename) in [
        ("contracts", "contract.json"),
        ("openapi", "openapi.json"),
        ("asyncapi", "asyncapi.json"),
    ] {
        let source = root.join(".forklaunch").join(kind).join(old);
        let destination = root.join(".forklaunch").join(kind).join(new);
        if !source.exists() {
            continue;
        }
        rename(&source, &destination)?;
        let path = destination.join(filename);
        if path.exists() && !(kind == "asyncapi" && imported) {
            let mut artifact: JsonValue = serde_json::from_str(&read_to_string(&path)?)?;
            if let Some(name) = artifact.pointer_mut("/service/name") {
                *name = json!(new);
            }
            if let Some(title) = artifact.pointer_mut("/info/title") {
                if title.as_str() == Some(old) {
                    *title = json!(new);
                }
            }
            write(&path, serde_json::to_string_pretty(&artifact)?)?;
        }
    }
    crate::sdk::websocket::rename_service(
        root,
        old,
        new,
        &mut StandardStream::stdout(ColorChoice::Always),
    )?;
    let old_member = crate::sdk::sdk_base(root)?
        .join(format!("rust/{old}-sdk"))
        .to_string_lossy()
        .into_owned();
    if root.join(&old_member).exists() {
        remove_cargo_member(root, &old_member)?;
        remove_path_if_exists(&root.join(&old_member))?;
        let source = if root
            .join(artifact_paths::application_path(root, new, Artifact::Contract))
            .exists()
        {
            "contract"
        } else {
            "manifest"
        };
        crate::sdk::generate::generate_or_check_sdk_outputs(
            root,
            "rust",
            source,
            Some(new),
            false,
            &mut StandardStream::stdout(ColorChoice::Always),
        )?;
    }
    crate::sdk::generate::cleanup_service_generated_files(root, old)?;
    if local_artifacts {
        return crate::sdk::generate::generate_or_check_sdk_outputs(
            root, "all", "contract", Some(new), false,
            &mut StandardStream::stdout(ColorChoice::Always),
        );
    }
    crate::sdk::generate::regenerate_sdk_outputs_if_present(
        root,
        &mut StandardStream::stdout(ColorChoice::Always),
    )
}

pub(crate) fn change_rust_application(
    app_root: &Path,
    new_name: Option<&str>,
    description: Option<&str>,
    author: Option<&str>,
    license: Option<&str>,
    previous_name: Option<&str>,
    dryrun: bool,
) -> Result<String> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let current_name = manifest
        .get("app_name")
        .and_then(Value::as_str)
        .context("Rust application manifest is missing app_name")?
        .to_string();
    let target_name = new_name.unwrap_or(&current_name).to_string();
    // A mixed application's TypeScript rename has already written the new
    // name; it passes the name it replaced.
    let previous_name = previous_name.unwrap_or(&current_name).to_string();

    if dryrun {
        let compose = manifest.get("docker_compose_path").and_then(Value::as_str).unwrap_or("docker-compose.yaml");
        let mut planned = vec![".forklaunch/manifest.toml".to_string()];
        if author.is_some() || license.is_some() {
            planned.push("Cargo.toml".into());
        }
        if app_root.join("README.md").is_file() && (new_name.is_some() || description.is_some()) {
            planned.push("README.md".into());
        } else if new_name.is_some() {
            planned.push(format!("{}/README.md", manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src")));
        }
        if new_name.is_some() {
            planned.extend([compose.to_string(), ".env".into(), "<crate>/.env.local".into(), "<crate>/**/package.json".into()]);
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        crate::core::rendered_template::log_dryrun_plan(&mut stdout, &planned)?;
        return Ok(target_name);
    }

    if new_name.is_some() { language_images::preflight(app_root)?; }

    let root = manifest
        .as_table_mut()
        .context("ForkLaunch manifest root must be a TOML table")?;
    if new_name.is_some() {
        root.insert("app_name".to_string(), Value::String(target_name.clone()));
        if let Some(topology) = root.get_mut("project_peer_topology").and_then(Value::as_table_mut)
            && previous_name != target_name
            && let Some(group) = topology.remove(&previous_name)
        {
            topology.insert(target_name.clone(), group);
        }
        for project in root.get_mut("projects").and_then(Value::as_array_mut).into_iter().flatten() {
            if let Some(description) = project.get_mut("description")
                && let Some(text) = description.as_str()
            {
                // Generated descriptions name the application ("Core library for <app>, …").
                let renamed = local_runtime::renamed_segment(text, &previous_name, &target_name);
                *description = Value::String(renamed);
            }
        }
    }
    if let Some(description) = description {
        root.insert(
            "app_description".to_string(),
            Value::String(description.to_string()),
        );
    }
    if let Some(author) = author {
        root.insert("author".to_string(), Value::String(author.to_string()));
    }
    if let Some(license) = license {
        root.insert("license".to_string(), Value::String(license.to_string()));
    }

    write_manifest_value(&manifest_path, &manifest)?;
    update_rust_root_cargo_metadata(app_root, author, license)?;
    update_rust_readme(app_root, &previous_name, &target_name, description)?;
    if new_name.is_some() {
        local_runtime::rename_application(app_root, &previous_name, &target_name)?;
        language_images::reconcile(app_root)?;
    }

    Ok(target_name)
}

pub(crate) fn change_rust_router(
    app_root: &Path,
    project_name: &str,
    existing_name: &str,
    new_name: &str,
    dryrun: bool,
) -> Result<()> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let crate_path = crate_path_for_project(&manifest, project_name).unwrap_or_else(|| {
        Path::new("crates")
            .join(project_name)
            .to_string_lossy()
            .to_string()
    });
    let native_dir = app_root.join(&crate_path);
    if contracts_layout::is_native_service(&native_dir) {
        native_layout::change_router(
            &native_dir,
            existing_name,
            Some(new_name),
            dryrun,
            &mut StandardStream::stdout(ColorChoice::Always),
        )?;
        if !dryrun {
            replace_router_in_manifest_value(&mut manifest, project_name, existing_name, new_name)?;
            write_manifest_value(&manifest_path, &manifest)?;
        }
        return Ok(());
    }
    let routes_dir = service_routes_directory(&app_root.join(&crate_path));
    let old_module = existing_name.to_case(Case::Snake);
    let new_module = new_name.to_case(Case::Snake);

    if !dryrun {
        let old_file = routes_dir.join(format!("{old_module}.rs"));
        let new_file = routes_dir.join(format!("{new_module}.rs"));
        if old_file.exists() {
            rename(&old_file, &new_file).with_context(|| {
                format!(
                    "Failed to rename {} to {}",
                    old_file.display(),
                    new_file.display()
                )
            })?;
        }
        write(
            routes_dir.join(format!("{new_module}.rs")),
            if modules::module_kind(app_root, project_name)?.is_some() {
                // Preserve user handlers when renaming a module extension.
                let source = read_to_string(&new_file)?;
                let old_response = format!("{}Response", existing_name.to_case(Case::Pascal));
                let new_response = format!("{}Response", new_name.to_case(Case::Pascal));
                regex::Regex::new(&format!(r"\b{}\b", regex::escape(&old_response)))?
                    .replace_all(&source, new_response.as_str())
                    .into_owned()
                    .replace(
                        &format!("\"{}\"", escape_rust_string(existing_name)),
                        &format!("\"{}\"", escape_rust_string(new_name)),
                    )
                    .replace(
                        &format!("\"/{}\"", escape_rust_string(existing_name)),
                        &format!("\"/{}\"", escape_rust_string(new_name)),
                    )
                    .replace(
                        &format!("Get {}", existing_name.to_case(Case::Title)),
                        &format!("Get {}", new_name.to_case(Case::Title)),
                    )
            } else {
                router_rs(new_name)
            },
        )
        .with_context(|| format!("Failed to update Rust router {new_name}"))?;
        let project_dir = app_root.join(&crate_path);
        let shared_module = shared_module_contracts(&project_dir);
        let contracts_dir = if shared_module { project_dir.join("api/contracts/extensions") } else { project_dir.join("src/contracts") };
        if shared_module {
            let source = read_to_string(&new_file)?.replace(&format!("extensions::{old_module}::"), &format!("extensions::{new_module}::"));
            write(&new_file, source)?;
        }
        if contracts_dir.join("mod.rs").exists() {
            let old_contract = contracts_dir.join(format!("{old_module}.rs"));
            let new_contract = contracts_dir.join(format!("{new_module}.rs"));
            if old_contract.exists() {
                let source = read_to_string(&old_contract)?;
                let old_response = format!("{}Response", existing_name.to_case(Case::Pascal));
                let new_response = format!("{}Response", new_name.to_case(Case::Pascal));
                let source = regex::Regex::new(&format!(r"\b{}\b", regex::escape(&old_response)))?
                    .replace_all(&source, new_response.as_str())
                    .into_owned()
                    .replace(&format!("/{existing_name}"), &format!("/{new_name}"))
                    .replace(&format!("\"{existing_name}\""), &format!("\"{new_name}\""))
                    .replace(&format!("Get {}", existing_name.to_case(Case::Title)), &format!("Get {}", new_name.to_case(Case::Title)))
                    .replace(
                        &format!("List {}", existing_name.to_case(Case::Title)),
                        &format!("List {}", new_name.to_case(Case::Title)),
                    );
                rename(&old_contract, &new_contract)?;
                write(new_contract, source)?;
            }
            if shared_module { rewrite_module_extensions(&contracts_dir, false, &mut StandardStream::stdout(ColorChoice::Always))?; } else { rewrite_contracts_mod(&contracts_dir)?; }
        }
        rewrite_routes_mod(
            &routes_dir,
            false,
            &mut StandardStream::stdout(ColorChoice::Always),
        )?;
        replace_router_in_manifest_value(&mut manifest, project_name, existing_name, new_name)?;
        write_manifest_value(&manifest_path, &manifest)?;
    }

    Ok(())
}

pub(crate) fn delete_rust_router(
    app_root: &Path,
    project_name: &str,
    router_name: &str,
) -> Result<()> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let crate_path = crate_path_for_project(&manifest, project_name).unwrap_or_else(|| {
        Path::new("crates")
            .join(project_name)
            .to_string_lossy()
            .to_string()
    });
    let native_dir = app_root.join(&crate_path);
    if contracts_layout::is_native_service(&native_dir) {
        native_layout::change_router(
            &native_dir,
            router_name,
            None,
            false,
            &mut StandardStream::stdout(ColorChoice::Always),
        )?;
        remove_router_from_manifest_value(&mut manifest, project_name, router_name)?;
        write_manifest_value(&manifest_path, &manifest)?;
        return remove_rust_project_artifacts(app_root, project_name);
    }
    let routes_dir = service_routes_directory(&app_root.join(&crate_path));
    let router_module = router_name.to_case(Case::Snake);
    let route_file = routes_dir.join(format!("{router_module}.rs"));
    if route_file.exists() {
        remove_file(&route_file)
            .with_context(|| format!("Failed to remove {}", route_file.display()))?;
    }
    let project_dir = app_root.join(&crate_path);
        let shared_module = shared_module_contracts(&project_dir);
        let contracts_dir = if shared_module { project_dir.join("api/contracts/extensions") } else { project_dir.join("src/contracts") };
    if contracts_dir.join("mod.rs").exists() {
        let contract = contracts_dir.join(format!("{router_module}.rs"));
        if contract.exists() {
            remove_file(contract)?;
        }
        if shared_module { rewrite_module_extensions(&contracts_dir, false, &mut StandardStream::stdout(ColorChoice::Always))?; } else { rewrite_contracts_mod(&contracts_dir)?; }
    }
    rewrite_routes_mod(
        &routes_dir,
        false,
        &mut StandardStream::stdout(ColorChoice::Always),
    )?;
    remove_router_from_manifest_value(&mut manifest, project_name, router_name)?;
    write_manifest_value(&manifest_path, &manifest)?;
    remove_rust_project_artifacts(app_root, project_name)
}

fn generate_rust_project(matches: &ArgMatches, kind: RustProjectKind) -> Result<()> {
    let dryrun = matches.get_flag("dryrun");
    if kind == RustProjectKind::Service && option(matches, "database").is_some_and(|database| database != "postgresql") {
        bail!("Rust service storage currently supports explicit --database postgresql; omit --database for the in-memory starter");
    }
    let name = required_name(matches, kind.label())?;
    let description = option(matches, "description")
        .unwrap_or_else(|| format!("Generated {name} {}", kind.label()));
    let app_root = find_manifest_root(
        &matches
            .get_one::<String>("base_path")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir().context("Failed to read current directory")?),
    )?;
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let crate_path = crate_path_for_project(&manifest, &name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let modules = Path::new(manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src"));
            if kind == RustProjectKind::Library {
                modules.join("core").join(&name)
            } else {
                modules.join(&name)
            }
        });
    let crate_dir = app_root.join(&crate_path);
    if let Some(project) = project_table(&manifest, &name) {
        if !is_rust_project(&app_root, &name)?
            || project.get("type").and_then(Value::as_str) != Some(kind.manifest_type())
            || project
                .get("variant")
                .and_then(Value::as_str)
                .and_then(modules::BuiltinModule::from_variant)
                .is_some()
        {
            bail!(
                "Project {name} already exists with a different project kind or module; use its change command"
            );
        }
    } else if crate_dir.join("Cargo.toml").exists() {
        bail!(
            "An existing crate occupies the destination for {name}; sync it before initialization"
        );
    }
    language_images::preflight(&app_root)?;
    mixed::ensure_workspace(&app_root, dryrun)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    ensure_embedded_framework(&app_root, dryrun)?;
    let requested_backend = worker::Backend::from_options(
        option(matches, "type").as_deref().unwrap_or("database"),
        option(matches, "database")
            .as_deref()
            .unwrap_or("postgresql"),
    );
    let backend = if kind == RustProjectKind::Worker {
        if crate_dir.join("Cargo.toml").exists() {
            worker::backend(&app_root, &name)?
        } else {
            requested_backend
        }
    } else {
        None
    };
    let native_layout = !crate_dir.join("src/lib.rs").exists();
    let postgres = kind == RustProjectKind::Service && (option(matches, "database").as_deref() == Some("postgresql") || crate_dir.join("persistence/database.rs").is_file());
    if postgres && (!native_layout || (crate_dir.join("Cargo.toml").is_file() && !crate_dir.join("persistence/database.rs").is_file())) {
        bail!("Choose --database postgresql when initializing a new native service; existing storage requires an explicit migration");
    }
    let new_project = !crate_dir.join("Cargo.toml").exists();
    let existing_service = crate_dir.join("consumer/lib.rs").is_file();
    let consumer_only = matches.try_get_one::<bool>("consumer-only").ok().flatten().copied().unwrap_or(false);
    if existing_service && consumer_only { bail!("Removing an existing worker service requires an explicit process migration"); }
    let worker_service = kind == RustProjectKind::Worker && native_layout && (existing_service || (new_project && !consumer_only));
    let worker_contracts = kind == RustProjectKind::Worker && native_layout && backend.is_some() && (new_project || existing_service || contracts_layout::directory(&crate_dir).is_some());
    if worker_contracts && new_project && contracts_layout::new_host_directory(&crate_dir).exists() { bail!("The worker contract destination already exists; choose another project name"); }
    let existing_apalis=crate_dir.join("worker_hooks.rs").is_file() || crate_dir.join("consumer/worker_hooks.rs").is_file();
    let apalis=kind==RustProjectKind::Worker && option(matches,"worker-runtime").map_or(existing_apalis,|runtime|runtime=="apalis");
    if kind==RustProjectKind::Worker && (!new_project && apalis!=existing_apalis) { bail!("Changing worker runtime requires an explicit queue migration"); }
    if apalis && (!native_layout || backend!=Some(worker::Backend::Postgres)) { bail!("Apalis requires a native PostgreSQL worker"); }

    if new_project && native_layout && (kind == RustProjectKind::Service || worker_service) {
        let host = contracts_layout::new_host_directory(&crate_dir);
        anyhow::ensure!(!host.exists(), "Contract package destination already exists: {}", host.display());
    }

    let templates = if kind == RustProjectKind::Service && native_layout {
        let templates = native_layout::service_templates(&crate_dir, &name, &description);
        if postgres {
            service_database::templates(&crate_dir, &name, templates)?
        } else { templates }
    } else if apalis {
        worker::apalis_templates(&app_root,&crate_dir,&name,&description)?
    } else if let Some(backend) = backend {
        worker::templates(&app_root, &crate_dir, &name, &description, backend)?
    } else {
        rust_crate_templates(&crate_dir, &name, &description, kind)
    };

    let templates = if native_layout && kind != RustProjectKind::Service && !apalis {
        native_layout::flatten(&crate_dir, &name, kind, templates)
    } else {
        templates
    };
    let templates = if worker_service { worker::service_templates(&crate_dir, &name, &description, templates, apalis, backend.context("Worker service requires a supported queue backend")?)? } else if worker_contracts { worker::consumer_contracts(&crate_dir,&name,templates)? } else { templates };
    let templates = if native_layout { native_mappers::templates(&crate_dir, matches.try_get_one::<bool>("mappers").ok().flatten().copied().unwrap_or(false), templates)? } else { templates };
    let templates = native_layout::framework_paths(&app_root, templates);
    let templates = if native_layout {
        templates
            .into_iter()
            .filter(|template| !template.path.exists())
            .collect()
    } else {
        templates
    };
    write_rust_rendered_templates(&templates, dryrun, &mut stdout)?;
    add_cargo_member(&app_root, &crate_path.to_string_lossy(), dryrun)?;
    if native_layout && kind == RustProjectKind::Service {
        add_cargo_member(
            &app_root,
            &contracts_layout::directory(&crate_dir).unwrap_or_else(|| contracts_layout::new_host_directory(&crate_dir)).strip_prefix(&app_root)?.to_string_lossy(),
            dryrun,
        )?;
    }
    if worker_contracts { add_cargo_member(&app_root, &contracts_layout::directory(&crate_dir).unwrap_or_else(|| contracts_layout::new_host_directory(&crate_dir)).strip_prefix(&app_root)?.to_string_lossy(), dryrun)?; }
    if (kind == RustProjectKind::Service || worker_service) && contracts_layout::new_host_directory(&crate_dir).join("Cargo.toml").is_file() {
        add_cargo_member(&app_root, &format!("{}/sdk/rust", crate_path.display()), dryrun)?;
    }
    add_project_to_manifest(&app_root, kind, &name, &description, matches, dryrun)?;
    if !dryrun {
        let path = app_root.join(".forklaunch/manifest.toml");
        let mut manifest = read_manifest_value(&path)?;
        if let Some(project) = project_table_mut(&mut manifest, &name) {
            if contracts_layout::new_host_directory(&crate_dir).join("Cargo.toml").is_file() && (kind == RustProjectKind::Service || worker_service) {
                project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?.insert("sdkLayout".into(),Value::String("project-local-v1".into()));
            }
            if kind==RustProjectKind::Worker { project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?.insert("workerService".into(), Value::Boolean(worker_service)); }
            if kind==RustProjectKind::Worker && (new_project || existing_service) { project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?.insert("contractExport".into(), Value::Boolean(worker_service)); }
            if kind==RustProjectKind::Worker { project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?.insert("workerRuntime".into(),Value::String(if apalis{"apalis"}else{"leased"}.into())); }
            if new_project && kind == RustProjectKind::Service {
                if !postgres { if let Some(resources)=project.get_mut("resources").and_then(Value::as_table_mut) { resources.remove("database"); } }
                project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?.insert("persistence".into(), Value::String(if postgres { "postgresql" } else { "memory" }.into()));
            }
            project
                .get_mut("metadata")
                .and_then(Value::as_table_mut)
                .context("missing metadata")?
                .insert(
                    "cratePath".into(),
                    Value::String(crate_path.to_string_lossy().into_owned()),
                );
            if contracts_layout::is_native_service(&crate_dir) {
                contracts_layout::set_artifact_paths(
                    project.get_mut("metadata").and_then(Value::as_table_mut).context("missing metadata")?,
                    &name,
                );
            }
        }
        write_manifest_value(&path, &manifest)?;
    }
    ensure_rust_local_library_dependencies(&app_root, dryrun)?;
    ensure_rust_docker_compose_project(&app_root, kind, &name, dryrun)?;
    ensure_rust_package_scripts(&app_root, kind, &name, dryrun)?;
    if !dryrun && postgres { service_database::compose(&app_root, &name)?; }
    if !dryrun && let Some(backend) = backend {
        worker::mark(&app_root, &name, backend)?;
        worker::compose(&app_root, &name, backend)?;
    }
    if !dryrun && kind != RustProjectKind::Library {
        ensure_rust_local_env(&app_root, &crate_dir, &name)?;
    }
    if kind == RustProjectKind::Service
        && let Some(desired) = crate::infra::native_local::requested(matches)
    {
        if dryrun {
            for id in desired {
                println!("Would configure {id} infrastructure for {name}");
            }
        } else {
            crate::infra::native_local::reconcile(matches, &app_root, &name, &desired, false)?;
        }
    }

    if !dryrun { language_images::reconcile(&app_root)?; }
    log_ok!(
        stdout,
        "Generated Rust {} {} at {}",
        kind.label(),
        name,
        crate_dir.display()
    );

    Ok(())
}

fn rust_crate_templates(
    crate_dir: &Path,
    name: &str,
    description: &str,
    kind: RustProjectKind,
) -> Vec<RenderedTemplate> {
    match kind {
        RustProjectKind::Service => service_templates(crate_dir, name, description),
        RustProjectKind::Worker => worker_templates(crate_dir, name, description),
        RustProjectKind::Library => library_templates(crate_dir, name, description),
    }
}

fn service_templates(crate_dir: &Path, name: &str, description: &str) -> Vec<RenderedTemplate> {
    let crate_module = name.to_case(Case::Snake);
    vec![
        RenderedTemplate {
            path: crate_dir.join("src/persistence/compliance_schema.rs"),
            content: render_static_rust_template("rust/project/service/src/persistence/compliance_schema.rs"),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("build.rs"),
            content: render_static_rust_template("rust/project/service/build.rs"),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src/contracts/mod.rs"),
            content: contracts_mod_rs(&["health".to_string()]),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src/contracts/models.rs"),
            content: render_static_rust_template("rust/project/service/src/contracts/models.rs"),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src/contracts/health.rs"),
            content: render_static_rust_template("rust/project/service/src/contracts/health.rs"),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("Cargo.toml"),
            content: service_cargo_toml(name, description),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("lib.rs"),
            content: service_lib_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("main.rs"),
            content: service_main_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("config").join("mod.rs"),
            content: service_config_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("domain").join("mod.rs"),
            content: service_domain_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("error").join("mod.rs"),
            content: service_error_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("persistence").join("mod.rs"),
            content: service_persistence_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("routes").join("mod.rs"),
            content: routes_mod_rs(&["health".to_string()]),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("routes").join("health.rs"),
            content: health_route_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("tests").join("smoke.rs"),
            content: service_smoke_test_rs(&crate_module),
            context: None,
        },
    ]
}

fn worker_templates(crate_dir: &Path, name: &str, description: &str) -> Vec<RenderedTemplate> {
    let crate_module = name.to_case(Case::Snake);
    vec![
        RenderedTemplate {
            path: crate_dir.join("Cargo.toml"),
            content: project_cargo_toml(name, description, true),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("lib.rs"),
            content: worker_lib_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("main.rs"),
            content: worker_main_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("config").join("mod.rs"),
            content: worker_config_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("error").join("mod.rs"),
            content: worker_error_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("jobs").join("mod.rs"),
            content: worker_jobs_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("tests").join("smoke.rs"),
            content: worker_smoke_test_rs(&crate_module),
            context: None,
        },
    ]
}

fn library_templates(crate_dir: &Path, name: &str, description: &str) -> Vec<RenderedTemplate> {
    let crate_module = name.to_case(Case::Snake);
    vec![
        RenderedTemplate {
            path: crate_dir.join("Cargo.toml"),
            content: project_cargo_toml(name, description, false),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("lib.rs"),
            content: library_lib_rs(name),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("error").join("mod.rs"),
            content: library_error_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("models").join("mod.rs"),
            content: library_models_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("src").join("service").join("mod.rs"),
            content: library_service_rs(),
            context: None,
        },
        RenderedTemplate {
            path: crate_dir.join("tests").join("smoke.rs"),
            content: library_smoke_test_rs(&crate_module),
            context: None,
        },
    ]
}

fn rust_crate_template_data(name: &str, description: &str, has_binary: bool) -> RustCrateTemplate {
    RustCrateTemplate {
        name: escape_toml_string(name),
        description: escape_toml_string(description),
        crate_module: name.to_case(Case::Snake),
        has_binary,
        uses_framework: has_binary,
        framework_rs_path: framework_rs_path(),
    }
}

fn framework_rs_path() -> String {
    "../../.forklaunch/framework-rs".to_string()
}

/// Framework and blueprint crates come from the `[framework]` selection: the
/// registry, or an external checkout referenced in place. Applications without
/// a selection get the published release.
pub(crate) fn ensure_embedded_framework(app_root: &Path, dryrun: bool) -> Result<()> {
    if dryrun {
        return Ok(());
    }
    dependencies::prepare(app_root)
}

fn service_routes_directory(directory: &Path) -> PathBuf {
    if shared_module_contracts(directory) {
        // Module routes bind the module's own contracts; added routers sit beside them.
        directory.join("api/routes/extensions")
    } else if directory.join("lib.rs").exists() {
        directory.join("api/routes")
    } else {
        directory.join("src/routes")
    }
}

fn service_lib_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/service/src/lib.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn service_main_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/service/src/main.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn service_config_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/service/src/config/mod.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn service_domain_rs() -> String {
    render_static_rust_template("rust/project/service/src/domain/mod.rs")
}

fn service_error_rs() -> String {
    render_static_rust_template("rust/project/service/src/error/mod.rs")
}

fn service_persistence_rs() -> String {
    render_static_rust_template("rust/project/service/src/persistence/mod.rs")
}

fn health_route_rs() -> String {
    render_static_rust_template("rust/project/service/src/routes/health.rs")
}

fn routes_mod_rs(modules: &[String]) -> String {
    let mut modules = modules.to_vec();
    modules.sort();
    modules.dedup();
    if let Some(health_index) = modules.iter().position(|module| module == "health") {
        let health = modules.remove(health_index);
        modules.insert(0, health);
    }

    render_rust_template(
        "rust/router/routes_mod.rs",
        &RustRoutesModTemplate {
            has_health: modules.iter().any(|module| module == "health"),
            modules: modules
                .into_iter()
                .map(|module| RustRouteModuleTemplate { module })
                .collect(),
        },
    )
}

fn contracts_mod_rs(modules: &[String]) -> String {
    let mut modules = modules.to_vec();
    modules.sort();
    modules.dedup();
    render_rust_template(
        "rust/project/service/src/contracts/mod.rs",
        &RustRoutesModTemplate {
            has_health: false,
            modules: modules
                .into_iter()
                .map(|module| RustRouteModuleTemplate { module })
                .collect(),
        },
    )
}

fn router_contract_rs(router_name: &str) -> String {
    render_rust_template(
        "rust/router/contract.rs",
        &RustRouterTemplate {
            router_module: router_name.to_case(Case::Snake),
            router_name: escape_rust_string(router_name),
            route_title: router_name.to_case(Case::Title),
            response_name: format!("{}Response", router_name.to_case(Case::Pascal)),
        },
    )
}

fn native_routes_mod_rs(modules: &[String]) -> String {
    let mut modules = modules.to_vec();
    modules.sort();
    modules.dedup();
    render_rust_template(
        "rust/router/native_routes_mod.rs",
        &RustRoutesModTemplate {
            has_health: false,
            modules: modules
                .into_iter()
                .map(|module| RustRouteModuleTemplate { module })
                .collect(),
        },
    )
}

fn native_router_rs(router_name: &str) -> String {
    render_rust_template(
        "rust/router/native_router.rs",
        &RustRouterTemplate {
            router_module: router_name.to_case(Case::Snake),
            router_name: escape_rust_string(router_name),
            route_title: router_name.to_case(Case::Title),
            response_name: format!("{}Response", router_name.to_case(Case::Pascal)),
        },
    )
}

fn router_rs(router_name: &str) -> String {
    render_rust_template(
        "rust/router/router.rs",
        &RustRouterTemplate {
            router_module: router_name.to_case(Case::Snake),
            router_name: escape_rust_string(router_name),
            route_title: router_name.to_case(Case::Title),
            response_name: format!("{}Response", router_name.to_case(Case::Pascal)),
        },
    )
}

fn service_smoke_test_rs(crate_module: &str) -> String {
    render_rust_template(
        "rust/project/service/tests/smoke.rs",
        &RustCrateTemplate {
            name: String::new(),
            description: String::new(),
            crate_module: crate_module.to_string(),
            has_binary: false,
            uses_framework: false,
            framework_rs_path: framework_rs_path(),
        },
    )
}

fn worker_lib_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/worker/src/lib.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn worker_main_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/worker/src/main.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn worker_config_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/worker/src/config/mod.rs",
        &rust_crate_template_data(name, "", true),
    )
}

fn worker_error_rs() -> String {
    render_static_rust_template("rust/project/worker/src/error/mod.rs")
}

fn worker_jobs_rs() -> String {
    render_static_rust_template("rust/project/worker/src/jobs/mod.rs")
}

fn worker_smoke_test_rs(crate_module: &str) -> String {
    render_rust_template(
        "rust/project/worker/tests/smoke.rs",
        &RustCrateTemplate {
            name: String::new(),
            description: String::new(),
            crate_module: crate_module.to_string(),
            has_binary: false,
            uses_framework: false,
            framework_rs_path: framework_rs_path(),
        },
    )
}

fn library_lib_rs(name: &str) -> String {
    render_rust_template(
        "rust/project/library/src/lib.rs",
        &rust_crate_template_data(name, "", false),
    )
}

fn library_error_rs() -> String {
    render_static_rust_template("rust/project/library/src/error/mod.rs")
}

fn library_models_rs() -> String {
    render_static_rust_template("rust/project/library/src/models/mod.rs")
}

fn library_service_rs() -> String {
    render_static_rust_template("rust/project/library/src/service/mod.rs")
}

fn library_smoke_test_rs(crate_module: &str) -> String {
    render_rust_template(
        "rust/project/library/tests/smoke.rs",
        &RustCrateTemplate {
            name: String::new(),
            description: String::new(),
            crate_module: crate_module.to_string(),
            has_binary: false,
            uses_framework: false,
            framework_rs_path: framework_rs_path(),
        },
    )
}

fn required_name(matches: &ArgMatches, label: &str) -> Result<String> {
    let name = if let Some(name) = matches.get_one::<String>("name") {
        name.clone()
    } else if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        let description = if matches!(label, "service" | "worker" | "library") {
            format!("Rust {label} crate name")
        } else {
            format!("Rust {label} name")
        };
        dialoguer::Input::<String>::new()
            .with_prompt(description)
            .default(format!("my-{label}"))
            .validate_with(|name: &String| {
                if validate_name(name) {
                    Ok(())
                } else {
                    Err("Name cannot be empty or include numbers or spaces")
                }
            })
            .interact_text()
            .with_context(|| format!("Failed to read {label} name"))?
    } else {
        bail!("Missing {label} name; pass it as a positional argument in non-interactive use");
    };
    if !validate_name(&name) {
        bail!("{label} name cannot be empty or include numbers or spaces");
    }
    Ok(name)
}

fn option(matches: &ArgMatches, id: &str) -> Option<String> {
    matches.try_get_one::<String>(id).ok().flatten().cloned()
}

fn render_rust_template<T: Content>(template_path: &str, data: &T) -> String {
    render_embedded_template(template_path, data)
        .unwrap_or_else(|error| panic!("Invalid embedded Rust template {template_path}: {error:#}"))
}

pub(super) fn render_static_rust_template(template_path: &str) -> String {
    render_rust_template(template_path, &EmptyRustTemplate {})
}

fn root_cargo_toml(license: &str, author: &str) -> String {
    let authors = if author.trim().is_empty() {
        "[]".to_string()
    } else {
        format!("[\"{}\"]", escape_toml_string(author))
    };
    render_rust_template(
        "rust/application/Cargo.toml.template",
        &RustRootCargoTemplate {
            license: escape_toml_string(license),
            authors,
        },
    )
}

fn project_cargo_toml(name: &str, description: &str, has_binary: bool) -> String {
    render_rust_template(
        "rust/project/Cargo.toml.template",
        &rust_crate_template_data(name, description, has_binary),
    )
}

fn service_cargo_toml(name: &str, description: &str) -> String {
    let base = project_cargo_toml(name, description, true).replace("[dependencies]", "[dependencies]\nserde_json = {}");
    format!(
        "{base}\n[build-dependencies]\nforklaunch-core = {{ path = \"../../.forklaunch/framework-rs/crates/core\", features = [\"compliance-artifact\", \"typed-schema\", \"build\"] }}\n"
    )
}

pub(crate) fn application_manifest(
    name: &str,
    description: &str,
    author: &str,
    license: &str,
    language: &str,
    sdk_targets: &[String],
) -> String {
    render_rust_template(
        "rust/application/manifest.toml",
        &RustApplicationManifestTemplate {
            id: Uuid::new_v4().to_string(),
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            name: escape_toml_string(name),
            description: escape_toml_string(description),
            author: escape_toml_string(author),
            license: escape_toml_string(license),
            language: language.to_string(),
            sdk_targets: sdk_targets_toml_array(sdk_targets),
        },
    )
}

fn mixed_typescript_sdk_templates(root_path: &Path, name: &str, sdk_targets: &[String]) -> Vec<RenderedTemplate> {
    let target = if sdk_targets.iter().any(|target| target == "rust") { "all" } else { "typescript" };
    let package_name = name.to_case(Case::Kebab);
    vec![
        RenderedTemplate {
            path: root_path.join("package.json"),
            content: serde_json::to_string_pretty(&json!({
                "name": package_name,
                "private": true,
                "type": "module",
                "packageManager": "pnpm@11.28.7",
                "scripts": {
                    "dev": "forklaunch dev",
                    "rust:build": "cargo build --workspace",
                    "rust:test": "cargo test --workspace",
                    "rust:fmt": "cargo fmt",
                    "rust:lint": "cargo clippy --workspace --all-targets --no-deps",
                    "sdk:generate": format!("forklaunch sdk generate --target {target} --from contract"),
                    "sdk:check": format!("forklaunch sdk generate --target {target} --from contract --check"),
                    "sdk:typecheck": "pnpm --filter '@forklaunch-generated/*' --if-present build",
                    "sdk:watch": format!("forklaunch sdk watch --target {target} --from contract")
                }
            }))
            .unwrap(),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("pnpm-workspace.yaml"),
            content: "packages:\n  - \"sdks/typescript\"\n  - \"src/client-sdk/events/typescript\"\n  - \"src/*/events/typescript\"\n".to_string(),
            context: None,
        },
        RenderedTemplate {
            path: root_path
                .join("sdks")
                .join("typescript")
                .join("package.json"),
            content: serde_json::to_string_pretty(&json!({
                "name": "@forklaunch-generated/sdk",
                "version": "0.0.0",
                "private": true,
                "type": "module",
                "exports": {
                    ".": "./src/index.ts"
                },
                "scripts": {
                    "build": "tsc --noEmit"
                },
                "devDependencies": {
                    "typescript": "^5.0.0"
                }
            }))
            .unwrap(),
            context: None,
        },
        RenderedTemplate {
            path: root_path.join("sdks").join("typescript").join("tsconfig.json"),
            content: TYPESCRIPT_SDK_TSCONFIG.to_owned(),
            context: None,
        },
        RenderedTemplate {
            path: root_path
                .join("sdks")
                .join("typescript")
                .join("src")
                .join("index.ts"),
            content: "export type ForkLaunchSdk = Record<string, never>;\n\nexport const sdk = {} as ForkLaunchSdk;\n".to_string(),
            context: None,
        },
    ]
}

fn rust_docker_compose_yaml(app_name: &str) -> Result<String> {
    let mut compose = DockerCompose::default();
    ensure_rust_docker_network(&mut compose, app_name);
    yaml_to_string(&compose).context("Failed to render Rust docker-compose.yaml")
}

fn ensure_rust_docker_compose_project(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
    dryrun: bool,
) -> Result<()> {
    if dryrun || matches!(kind, RustProjectKind::Library) {
        return Ok(());
    }

    let compose_path = app_root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&compose_path)?;
    let app_name = rust_app_name(app_root)?;
    ensure_rust_docker_network(&mut compose, &app_name);

    let port = if matches!(kind, RustProjectKind::Service) {
        Some(next_rust_docker_port(&compose))
    } else {
        None
    };
    let dockerfile = mixed::dockerfile(app_root)?;
    compose.services.entry(name.to_string()).or_insert_with(|| {
        let mut service = rust_docker_service(&app_name, name, kind, port);
        if let Some(build) = &mut service.build {
            build.dockerfile = dockerfile.to_owned();
        }
        service
    });

    write(&compose_path, yaml_to_string(&compose)?)
        .with_context(|| format!("Failed to update {}", compose_path.display()))
}

/// Local `cargo run` settings for a project's HTTP listener, matching its
/// Compose port. Settings are required at startup, so they are written out;
/// existing values are kept and the file stays git-ignored.
fn ensure_rust_local_env(app_root: &Path, crate_dir: &Path, name: &str) -> Result<()> {
    let compose = read_or_create_rust_docker_compose(&app_root.join("docker-compose.yaml"))?;
    let Some(port) = [name.to_owned(), format!("{name}-server")]
        .iter()
        .filter_map(|service| compose.services.get(service))
        .flat_map(|service| service.ports.as_deref().unwrap_or_default())
        .find_map(|port| crate::core::docker::compose_host_port(port))
    else {
        return Ok(());
    };
    let path = crate_dir.join(".env.local");
    let existing = read_to_string(&path).unwrap_or_default();
    let mut content = existing.clone();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    let port = port.to_string();
    for (key, value) in [
        ("HOST", "127.0.0.1"),
        ("PORT", port.as_str()),
        ("PROTOCOL", "http"),
        ("VERSION", "v1"),
        ("DOCS_PATH", "/docs"),
        ("RUST_LOG", "info"),
        ("MCP_ENABLED", "true"),
    ] {
        let present = existing
            .lines()
            .any(|line| line.split_once('=').is_some_and(|(existing, _)| existing.trim() == key));
        if !present {
            content.push_str(&format!("{key}={value}\n"));
        }
    }
    if content != existing {
        write(&path, content).with_context(|| format!("Failed to update {}", path.display()))?;
    }
    Ok(())
}

fn remove_rust_docker_compose_project(app_root: &Path, name: &str) -> Result<()> {
    let compose_path = app_root.join("docker-compose.yaml");
    if !compose_path.exists() {
        return Ok(());
    }
    let mut compose = read_or_create_rust_docker_compose(&compose_path)?;
    compose.services.shift_remove(name);
    // The project's one-shot migration job goes with it.
    compose.services.shift_remove(&format!("{name}-migrate"));
    compose
        .services
        .retain(|_, service| modules::ecommerce_worker_owner(service) != Some(name) && worker::producer_owner(service) != Some(name));
    // Nothing may wait on a removed process.
    let removed: Vec<String> = [name.to_owned(), format!("{name}-migrate"), format!("{name}-server"), format!("{name}-seed")]
        .into_iter()
        .filter(|key| !compose.services.contains_key(key) || key.ends_with("-migrate"))
        .collect();
    for service in compose.services.values_mut() {
        if let Some(depends) = service.depends_on.as_mut() {
            for key in &removed {
                depends.shift_remove(key);
            }
            if depends.is_empty() {
                service.depends_on = None;
            }
        }
    }
    let seed = format!("{name}-seed");
    if compose.services.get(&seed).is_some_and(|service| modules::is_seed_job(service, name)) {
        compose.services.shift_remove(&seed);
    }
    write(&compose_path, yaml_to_string(&compose)?)
        .with_context(|| format!("Failed to update {}", compose_path.display()))
}

fn read_or_create_rust_docker_compose(compose_path: &Path) -> Result<DockerCompose> {
    if compose_path.exists() {
        let content = read_to_string(compose_path)
            .with_context(|| format!("Failed to read {}", compose_path.display()))?;
        yaml_from_str::<DockerCompose>(&content)
            .with_context(|| format!("Failed to parse {}", compose_path.display()))
    } else {
        Ok(DockerCompose::default())
    }
}

fn ensure_rust_docker_network(compose: &mut DockerCompose, app_name: &str) {
    let network_name = format!("{app_name}-network");
    compose
        .networks
        .entry(network_name.clone())
        .or_insert_with(|| DockerNetwork {
            name: network_name,
            driver: "bridge".to_string(),
            ..Default::default()
        });
}

fn rust_docker_service(
    app_name: &str,
    name: &str,
    kind: RustProjectKind,
    port: Option<i32>,
) -> DockerService {
    let mut environment = IndexMap::new();
    environment.insert("HOST".to_string(), "0.0.0.0".to_string());
    environment.insert("PROTOCOL".to_string(), "http".to_string());
    environment.insert("VERSION".to_string(), "v1".to_string());
    environment.insert("DOCS_PATH".to_string(), "/docs".to_string());
    environment.insert("RUST_LOG".to_string(), "info".to_string());
    if let Some(port) = port {
        environment.insert("PORT".to_string(), port.to_string());
        environment.insert("MCP_ENABLED".to_string(), "true".to_string());
        environment.insert(
            "PUBLIC_ORIGIN".to_string(),
            format!("http://localhost:{port}"),
        );
    }
    if matches!(kind, RustProjectKind::Worker) {
        environment.insert("QUEUE_NAME".to_string(), format!("{app_name}-{name}-dev"));
    }

    DockerService {
        hostname: Some(name.to_string()),
        container_name: Some(format!("{app_name}-{name}-rust")),
        image: Some(format!("{app_name}-{name}-rust:latest")),
        restart: Some(Restart::UnlessStopped),
        build: Some(DockerBuild {
            context: ".".to_string(),
            dockerfile: "./Dockerfile".to_string(),
            ..Default::default()
        }),
        environment: Some(environment),
        ports: port.map(|port| vec![format!("{port}:{port}")]),
        networks: Some(vec![format!("{app_name}-network")]),
        working_dir: Some("/app".to_string()),
        command: Some(DockerCommand::Multiple(vec![format!(
            "./target/release/{name}"
        )])),
        healthcheck: port.map(|port| Healthcheck {
            test: HealthTest::List(vec![
                "CMD".to_string(),
                "curl".to_string(),
                "-f".to_string(),
                format!("http://localhost:{port}/health"),
            ]),
            interval: "30s".to_string(),
            timeout: "10s".to_string(),
            retries: 3,
            start_period: "40s".to_string(),
            additional_properties: Default::default(),
        }),
        ..Default::default()
    }
}

fn next_rust_docker_port(compose: &DockerCompose) -> i32 {
    compose
        .services
        .values()
        .flat_map(|service| service.ports.as_deref().unwrap_or_default())
        .filter_map(|port| crate::core::docker::compose_host_port(port))
        .filter(|port| (8000..9000).contains(port))
        .max()
        .unwrap_or(7999)
        + 1
}

fn rust_app_name(app_root: &Path) -> Result<String> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let manifest = read_manifest_value(&manifest_path)?;
    Ok(manifest
        .get("app_name")
        .and_then(Value::as_str)
        .unwrap_or("forklaunch")
        .to_case(Case::Kebab))
}

fn ensure_rust_package_scripts(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
    dryrun: bool,
) -> Result<()> {
    if dryrun {
        return Ok(());
    }

    // The first native project may create the root package/workspace here.
    sdk_typescript::reconcile(app_root)?;
    // The generated Rust `clients` aggregate re-exports every service SDK.
    sdk_consumer::reconcile_local_aggregate(app_root)?;
    let package_path = app_root.join("package.json");
    if !package_path.exists() {
        return Ok(());
    }

    let mut package = read_package_json(&package_path)?;
    let package_object = package
        .as_object_mut()
        .context("Root package.json must be a JSON object")?;
    let scripts = package_object
        .entry("scripts".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Root package.json scripts must be a JSON object")?;

    scripts.insert(
        "rust:build".to_string(),
        JsonValue::String("cargo build --workspace".to_string()),
    );
    scripts.insert(
        "rust:test".to_string(),
        JsonValue::String("cargo test --workspace".to_string()),
    );
    scripts.insert(
        "rust:fmt".to_string(),
        JsonValue::String("cargo fmt".to_string()),
    );
    scripts.insert(
        "rust:lint".to_string(),
        JsonValue::String("cargo clippy --workspace --all-targets --no-deps".to_string()),
    );
    scripts.insert(
        format!("build:{name}"),
        JsonValue::String(format!("cargo build -p {name}")),
    );
    scripts.insert(
        format!("test:{name}"),
        JsonValue::String(format!("cargo test -p {name}")),
    );
    if matches!(kind, RustProjectKind::Service | RustProjectKind::Worker) {
        scripts.insert(
            format!("dev:{name}"),
            JsonValue::String(format!("cargo run -p {name}")),
        );
    }
    if kind==RustProjectKind::Worker {
        let manifest=read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
        if project_table(&manifest,name).and_then(|project|project.get("metadata")).and_then(|metadata|metadata.get("workerService")).and_then(Value::as_bool)==Some(true) {
            scripts.insert(format!("dev:{name}:worker"),JsonValue::String(format!("cargo run -p {name} --bin {name}-worker")));
            scripts.insert(format!("dev:{name}:server"),JsonValue::String(format!("cargo run -p {name} --bin {name}")));
        }
    }

    let rendered = serde_json::to_string_pretty(&package)?;
    write(
        &package_path,
        match read_to_string(&package_path) {
            Ok(original) => crate::core::package_json::preserve_json_layout(&original, &rendered),
            Err(_) => rendered,
        },
    )
        .with_context(|| format!("Failed to update {}", package_path.display()))?;
    Ok(())
}

fn remove_rust_package_scripts(app_root: &Path, name: &str) -> Result<()> {
    let package_path = app_root.join("package.json");
    if !package_path.exists() {
        return Ok(());
    }

    let mut package = read_package_json(&package_path)?;
    if let Some(scripts) = package
        .get_mut("scripts")
        .and_then(JsonValue::as_object_mut)
    {
        for script in [
            format!("dev:{name}:worker"),
            format!("dev:{name}:server"),
            format!("dev:{name}"),
            format!("build:{name}"),
            format!("test:{name}"),
        ] {
            scripts.remove(&script);
        }
    }

    let rendered = serde_json::to_string_pretty(&package)?;
    write(
        &package_path,
        match read_to_string(&package_path) {
            Ok(original) => crate::core::package_json::preserve_json_layout(&original, &rendered),
            Err(_) => rendered,
        },
    )
        .with_context(|| format!("Failed to update {}", package_path.display()))?;
    sdk_typescript::reconcile(app_root)
}

fn read_package_json(package_path: &Path) -> Result<JsonValue> {
    serde_json::from_str::<JsonValue>(
        &read_to_string(package_path)
            .with_context(|| format!("Failed to read {}", package_path.display()))?,
    )
    .with_context(|| format!("Failed to parse {}", package_path.display()))
}

fn find_manifest_root(start: &Path) -> Result<PathBuf> {
    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .context("Failed to read current directory")?
            .join(start)
    };
    let mut cursor = if start.is_file() {
        start
            .parent()
            .context("Failed to resolve parent directory")?
            .to_path_buf()
    } else {
        start
    };

    loop {
        if cursor.join(".forklaunch").join("manifest.toml").exists() {
            return Ok(cursor);
        }
        if !cursor.pop() {
            bail!("Could not find .forklaunch/manifest.toml for Rust project generation");
        }
    }
}

fn edit_cargo_members(root: &Path, update: impl FnOnce(&mut toml_edit::Array)) -> Result<()> {
    let path = root.join("Cargo.toml");
    let original = read_to_string(&path)?;
    let mut cargo: toml_edit::DocumentMut = original.parse()?;
    let members = cargo["workspace"]["members"]
        .as_array_mut()
        .context("Expected workspace.members array")?;
    update(members);
    // Keep one member per line, as the generated workspace starts out.
    for member in members.iter_mut() {
        member.decor_mut().set_prefix("\n    ");
        member.decor_mut().set_suffix("");
    }
    members.set_trailing("\n");
    members.set_trailing_comma(true);
    let updated = cargo.to_string();
    if updated != original {
        write(path, updated)?;
    }
    Ok(())
}

fn add_cargo_member(app_root: &Path, member: &str, dryrun: bool) -> Result<()> {
    if dryrun {
        return Ok(());
    }
    edit_cargo_members(app_root, |members| {
        if !members.iter().any(|value| value.as_str() == Some(member)) {
            members.push(member);
        }
    })
}

fn add_project_to_manifest(
    app_root: &Path,
    kind: RustProjectKind,
    name: &str,
    description: &str,
    matches: &ArgMatches,
    dryrun: bool,
) -> Result<()> {
    if dryrun {
        return Ok(());
    }

    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let sdk_targets = sdk_targets_from_manifest(&manifest);
    let root = manifest
        .as_table_mut()
        .context("ForkLaunch manifest root must be a TOML table")?;
    let projects = root
        .entry("projects")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("ForkLaunch manifest projects must be an array")?;

    if !projects.iter().any(|project| {
        project
            .get("name")
            .and_then(Value::as_str)
            .map(|project_name| project_name == name)
            .unwrap_or(false)
    }) {
        projects.push(project_manifest_entry(
            kind,
            name,
            description,
            matches,
            &sdk_targets,
        ));
    }

    write_manifest_value(&manifest_path, &manifest)
}

fn add_router_to_manifest(
    app_root: &Path,
    service_name: &str,
    router_name: &str,
    dryrun: bool,
) -> Result<()> {
    if dryrun {
        return Ok(());
    }

    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let projects = manifest
        .as_table_mut()
        .context("ForkLaunch manifest root must be a TOML table")?
        .entry("projects")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("ForkLaunch manifest projects must be an array")?;

    let Some(project) = projects.iter_mut().find(|project| {
        project
            .get("name")
            .and_then(Value::as_str)
            .map(|project_name| project_name == service_name)
            .unwrap_or(false)
    }) else {
        bail!("Could not find project {service_name} in ForkLaunch manifest");
    };

    let project_table = project
        .as_table_mut()
        .context("ForkLaunch project entry must be a TOML table")?;
    let routers = project_table
        .entry("routers")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("ForkLaunch project routers must be an array")?;
    if !routers
        .iter()
        .any(|router| router.as_str() == Some(router_name))
    {
        routers.push(Value::String(router_name.to_string()));
    }

    write_manifest_value(&manifest_path, &manifest)
}

fn project_manifest_entry(
    kind: RustProjectKind,
    name: &str,
    description: &str,
    matches: &ArgMatches,
    sdk_targets: &[String],
) -> Value {
    let mut table = Map::new();
    let crate_path = format!("crates/{name}");
    table.insert(
        "type".to_string(),
        Value::String(kind.manifest_type().to_string()),
    );
    table.insert("name".to_string(), Value::String(name.to_string()));
    table.insert(
        "description".to_string(),
        Value::String(description.to_string()),
    );
    table.insert("routers".to_string(), Value::Array(Vec::new()));

    if let Some(resources) = resources_for_project(kind, matches) {
        table.insert("resources".to_string(), Value::Table(resources));
    }

    let mut metadata = Map::new();
    metadata.insert("language".to_string(), Value::String("rust".to_string()));
    metadata.insert("crateName".to_string(), Value::String(name.to_string()));
    metadata.insert("cratePath".to_string(), Value::String(crate_path));
    metadata.insert(
        "sdkTargets".to_string(),
        Value::Array(sdk_targets.iter().cloned().map(Value::String).collect()),
    );
    if matches!(kind, RustProjectKind::Worker)
        && let Some(worker_type) = option(matches, "type")
    {
        metadata.insert("workerType".to_string(), Value::String(worker_type));
    }
    table.insert("metadata".to_string(), Value::Table(metadata));

    Value::Table(table)
}

fn resources_for_project(
    kind: RustProjectKind,
    matches: &ArgMatches,
) -> Option<Map<String, Value>> {
    let mut resources = Map::new();
    match kind {
        RustProjectKind::Service => {
            let database = option(matches, "database").unwrap_or_else(|| "postgresql".to_string());
            resources.insert("database".to_string(), Value::String(database));
        }
        RustProjectKind::Worker => {
            let worker_type = option(matches, "type").unwrap_or_else(|| "database".to_string());
            match worker_type.as_str() {
                "database" => {
                    let database =
                        option(matches, "database").unwrap_or_else(|| "postgresql".to_string());
                    resources.insert("database".to_string(), Value::String(database));
                }
                "redis" | "bullmq" => {
                    resources.insert("cache".to_string(), Value::String("redis".to_string()));
                }
                "kafka" => {
                    resources.insert("queue".to_string(), Value::String("kafka".to_string()));
                }
                _ => {}
            }
        }
        RustProjectKind::Library => {}
    }

    if resources.is_empty() {
        None
    } else {
        Some(resources)
    }
}

fn project_table<'a>(manifest: &'a Value, name: &str) -> Option<&'a Map<String, Value>> {
    manifest
        .get("projects")
        .and_then(Value::as_array)?
        .iter()
        .find(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(|project_name| project_name == name)
                .unwrap_or(false)
        })?
        .as_table()
}

fn project_table_mut<'a>(
    manifest: &'a mut Value,
    name: &str,
) -> Option<&'a mut Map<String, Value>> {
    manifest
        .get_mut("projects")
        .and_then(Value::as_array_mut)?
        .iter_mut()
        .find(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(|project_name| project_name == name)
                .unwrap_or(false)
        })?
        .as_table_mut()
}

fn crate_path_for_project(manifest: &Value, name: &str) -> Option<String> {
    project_table(manifest, name)
        .and_then(|project| project.get("metadata"))
        .and_then(Value::as_table)
        .and_then(|metadata| metadata.get("cratePath"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn ensure_rust_local_library_dependencies(app_root: &Path, dryrun: bool) -> Result<()> {
    if dryrun {
        return Ok(());
    }

    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    if !manifest_path.exists() {
        return Ok(());
    }

    let manifest = read_manifest_value(&manifest_path)?;
    let projects = rust_workspace_projects(&manifest);
    let libraries = projects
        .iter()
        .filter(|project| project.kind == RustProjectKind::Library)
        .cloned()
        .collect::<Vec<_>>();

    for project in projects.iter().filter(|project| {
        matches!(
            project.kind,
            RustProjectKind::Service | RustProjectKind::Worker
        )
    }) {
        update_rust_project_local_library_dependencies(app_root, project, &libraries)?;
    }

    Ok(())
}

fn rust_workspace_projects(manifest: &Value) -> Vec<RustWorkspaceProject> {
    manifest
        .get("projects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let name = project.get("name").and_then(Value::as_str)?.to_string();
                    let kind = project
                        .get("type")
                        .and_then(Value::as_str)
                        .and_then(kind_from_manifest_type)?;
                    let metadata = project.get("metadata").and_then(Value::as_table)?;
                    if metadata.get("language").and_then(Value::as_str) != Some("rust") {
                        return None;
                    }
                    let crate_path = metadata
                        .get("cratePath")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("crates/{name}"));

                    Some(RustWorkspaceProject {
                        name,
                        kind,
                        crate_path,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn update_rust_project_local_library_dependencies(
    app_root: &Path,
    project: &RustWorkspaceProject,
    libraries: &[RustWorkspaceProject],
) -> Result<()> {
    let cargo_path = app_root.join(&project.crate_path).join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(());
    }

    let content = read_to_string(&cargo_path)
        .with_context(|| format!("Failed to read {}", cargo_path.display()))?;
    let mut cargo = toml::from_str::<Value>(&content)
        .with_context(|| format!("Failed to parse {}", cargo_path.display()))?;
    let root = cargo
        .as_table_mut()
        .context("Cargo.toml must be a TOML table")?;
    let dependencies = root
        .entry("dependencies")
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .context("Cargo.toml dependencies must be a TOML table")?;

    let library_names = libraries
        .iter()
        .map(|library| library.name.as_str())
        .collect::<HashSet<_>>();
    let library_paths = libraries
        .iter()
        .map(|library| normalize_path(app_root.join(&library.crate_path)))
        .collect::<HashSet<_>>();
    let crates_root = normalize_path(app_root.join("crates"));
    let project_dir = app_root.join(&project.crate_path);

    dependencies.retain(|dependency_name, dependency| {
        let package = dependency.get("package").and_then(Value::as_str).unwrap_or(dependency_name);
        if library_names.contains(package) {
            return true;
        }

        let Some(path) = dependency_path(dependency) else {
            return true;
        };
        let resolved_path = normalize_path(project_dir.join(path));
        if resolved_path.starts_with(&crates_root) {
            return library_paths.contains(&resolved_path);
        }
        true
    });

    for library in libraries {
        if library.name == project.name {
            continue;
        }
        // A relocated registered library leaves an unresolved local path behind.
        // Repair that reference by package identity, retaining aliases and flags.
        // Existing valid overrides and registry/git dependencies remain owned by the app.
        let mut referenced = false;
        for (alias, dependency) in dependencies.iter_mut() {
            let package = dependency.get("package").and_then(Value::as_str).unwrap_or(alias);
            if package != library.name { continue; }
            referenced = true;
            if let Some(path) = dependency_path(dependency)
                && !project_dir.join(path).join("Cargo.toml").is_file()
                && let Some(table) = dependency.as_table_mut()
            {
                table.insert("path".into(), Value::String(relative_cargo_path(&project.crate_path, &library.crate_path)));
            }
        }
        // Existing dependency flags and explicit aliases belong to the application.
        if referenced || dependencies.values().any(|dependency| {
            dependency_path(dependency).is_some_and(|path| {
                normalize_path(project_dir.join(path)) == normalize_path(app_root.join(&library.crate_path))
            })
        }) { continue; }
        let mut dependency = Map::new();
        dependency.insert(
            "path".to_string(),
            Value::String(relative_cargo_path(
                &project.crate_path,
                &library.crate_path,
            )),
        );
        dependencies.insert(library.name.clone(), Value::Table(dependency));
    }

    let updated = edited_dependencies(&content, &cargo)?;
    if updated != content {
        write(&cargo_path, updated).with_context(|| format!("Failed to update {}", cargo_path.display()))?;
    }
    Ok(())
}

/// `content` with its `[dependencies]` brought to those of `cargo`, editing
/// only the entries that changed so the rest of the manifest keeps its layout.
fn edited_dependencies(content: &str, cargo: &Value) -> Result<String> {
    let original: Value = toml::from_str(content)?;
    let empty = Map::new();
    let before = original.get("dependencies").and_then(Value::as_table).unwrap_or(&empty);
    let after = cargo.get("dependencies").and_then(Value::as_table).unwrap_or(&empty);
    if before == after {
        return Ok(content.to_owned());
    }
    let mut document: toml_edit::DocumentMut = content.parse()?;
    if !document.contains_key("dependencies") {
        document["dependencies"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let table = document["dependencies"].as_table_like_mut().context("Cargo dependencies")?;
    for key in before.keys().filter(|key| !after.contains_key(*key)) {
        table.remove(key);
    }
    for (key, value) in after.iter().filter(|(key, value)| before.get(*key) != Some(*value)) {
        let item: toml_edit::DocumentMut = format!("value = {value}").parse()?;
        table.insert(key, item["value"].clone());
    }
    Ok(document.to_string())
}

// Custom library paths are not necessarily under crates/. Remove only the
// dependency that still points to the deleted registration, preserving overrides.
fn remove_registered_library_dependency(
    app_root: &Path,
    manifest: &Value,
    name: &str,
    crate_path: &str,
) -> Result<()> {
    let removed_path = normalize_path(app_root.join(crate_path));
    for project in rust_workspace_projects(manifest)
        .into_iter()
        .filter(|project| {
            matches!(
                project.kind,
                RustProjectKind::Service | RustProjectKind::Worker
            )
        })
    {
        let project_dir = app_root.join(&project.crate_path);
        let cargo_path = project_dir.join("Cargo.toml");
        if !cargo_path.is_file() {
            continue;
        }
        let content = read_to_string(&cargo_path)?;
        let mut cargo: Value = toml::from_str(&content)?;
        let Some(dependencies) = cargo.get_mut("dependencies").and_then(Value::as_table_mut) else {
            continue;
        };
        let previous_count = dependencies.len();
        dependencies.retain(|alias, dependency| {
            let package = dependency.get("package").and_then(Value::as_str).unwrap_or(alias);
            package != name || !dependency_path(dependency)
                .is_some_and(|path| normalize_path(project_dir.join(path)) == removed_path)
        });
        if dependencies.len() != previous_count {
            write(&cargo_path, toml::to_string_pretty(&cargo)?)
                .with_context(|| format!("Failed to update {}", cargo_path.display()))?;
        }
    }
    Ok(())
}

fn dependency_path(dependency: &Value) -> Option<&str> {
    dependency
        .as_table()
        .and_then(|table| table.get("path"))
        .and_then(Value::as_str)
}

fn relative_cargo_path(from_crate_path: &str, to_crate_path: &str) -> String {
    let from = normal_relative_components(from_crate_path);
    let to = normal_relative_components(to_crate_path);
    let common = from
        .iter()
        .zip(to.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut path_parts = Vec::new();
    for _ in common..from.len() {
        path_parts.push("..".to_string());
    }
    path_parts.extend(to[common..].iter().cloned());

    if path_parts.is_empty() {
        ".".to_string()
    } else {
        path_parts.join("/")
    }
}

fn normal_relative_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str().map(str::to_string),
            _ => None,
        })
        .collect()
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn kind_from_manifest_type(manifest_type: &str) -> Option<RustProjectKind> {
    match manifest_type {
        "Service" => Some(RustProjectKind::Service),
        "Worker" => Some(RustProjectKind::Worker),
        "Library" => Some(RustProjectKind::Library),
        _ => None,
    }
}

fn detect_rust_project_kind(app_root: &Path, name: &str) -> Result<RustProjectKind> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let manifest = read_manifest_value(&manifest_path)?;
    if let Some(kind) = project_table(&manifest, name)
        .and_then(|project| project.get("type"))
        .and_then(Value::as_str)
        .and_then(kind_from_manifest_type)
    {
        return Ok(kind);
    }

    let crate_dir = app_root.join(discovery::path(app_root, &manifest, name)?.context("Missing Rust project path")?);
    if ["consumer/worker.rs", "consumer/worker_hooks.rs", "worker.rs", "src/jobs/mod.rs"].iter().any(|path| crate_dir.join(path).is_file()) {
        return Ok(RustProjectKind::Worker);
    }
    if ["api/routes/mod.rs", "src/routes/mod.rs"].iter().any(|path| crate_dir.join(path).is_file()) {
        return Ok(RustProjectKind::Service);
    }
    Ok(RustProjectKind::Library)
}

fn remove_project_from_manifest_value(
    manifest: &mut Value,
    name: &str,
    kind: Option<RustProjectKind>,
) -> Result<()> {
    let projects = manifest
        .as_table_mut()
        .context("ForkLaunch manifest root must be a TOML table")?
        .entry("projects")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("ForkLaunch manifest projects must be an array")?;
    let before_len = projects.len();
    projects.retain(|project| {
        let same_name = project
            .get("name")
            .and_then(Value::as_str)
            .map(|project_name| project_name == name)
            .unwrap_or(false);
        let same_kind = kind
            .map(|kind| {
                project
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|project_type| project_type == kind.manifest_type())
                    .unwrap_or(false)
            })
            .unwrap_or(true);
        !(same_name && same_kind)
    });
    if projects.len() == before_len {
        bail!("Could not find Rust project {name} in manifest");
    }
    Ok(())
}

fn cleanup_missing_rust_projects(app_root: &Path) -> Result<()> {
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let missing: Vec<_> = rust_workspace_projects(&manifest).into_iter()
        .filter(|project| !app_root.join(&project.crate_path).join("Cargo.toml").is_file())
        .collect();
    for project in &missing {
        delete_rust_project_inner(app_root, project.kind, &project.name, false)?;
    }
    if !missing.is_empty() {
        ensure_rust_local_library_dependencies(app_root, false)?;
        language_images::reconcile(app_root)?;
    }
    Ok(())
}

fn cargo_description(cargo_path: &Path) -> Result<Option<String>> {
    let content = read_to_string(cargo_path)
        .with_context(|| format!("Failed to read {}", cargo_path.display()))?;
    let value = toml::from_str::<Value>(&content)
        .with_context(|| format!("Failed to parse {}", cargo_path.display()))?;
    Ok(value
        .get("package")
        .and_then(Value::as_table)
        .and_then(|package| package.get("description"))
        .and_then(Value::as_str)
        .map(str::to_string))
}

pub(crate) fn remove_cargo_member(app_root: &Path, member: &str) -> Result<()> {
    if !app_root.join("Cargo.toml").exists() {
        return Ok(());
    }
    edit_cargo_members(app_root, |members| {
        members.retain(|value| value.as_str() != Some(member))
    })
}

/// A renamed TypeScript project keeps its generated Rust SDK crate (a root
/// workspace member) under the new directory.
pub(crate) fn rename_typescript_sdk_member(app_root: &Path, old_dir: &Path, new_dir: &Path) -> Result<()> {
    if !app_root.join("Cargo.toml").exists() {
        return Ok(());
    }
    let relative = |dir: &Path| -> Result<String> {
        Ok(dir.strip_prefix(app_root).unwrap_or(dir).join("sdk/rust").to_string_lossy().into_owned())
    };
    let (current, target) = (relative(old_dir)?, relative(new_dir)?);
    let listed = read_to_string(app_root.join("Cargo.toml"))?
        .parse::<toml_edit::DocumentMut>()?
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml_edit::Item::as_array)
        .is_some_and(|members| members.iter().any(|member| member.as_str() == Some(current.as_str())));
    if listed {
        replace_cargo_member(app_root, &current, &target)?;
    }
    Ok(())
}

/// A deleted TypeScript project's generated Rust SDK crate leaves the root workspace.
pub(crate) fn remove_typescript_sdk_member(app_root: &Path, dir: &Path) -> Result<()> {
    let member = dir.strip_prefix(app_root).unwrap_or(dir).join("sdk/rust");
    remove_cargo_member(app_root, &member.to_string_lossy())
}

fn replace_cargo_member(app_root: &Path, current_member: &str, target_member: &str) -> Result<()> {
    edit_cargo_members(app_root, |members| {
        for index in 0..members.len() {
            if members.get(index).and_then(toml_edit::Value::as_str) == Some(current_member) {
                members.replace(index, target_member);
            }
        }
        if !members
            .iter()
            .any(|value| value.as_str() == Some(target_member))
        {
            members.push(target_member);
        }
    })
}

fn update_package_name(cargo_path: &Path, name: &str) -> Result<()> {
    if !cargo_path.exists() {
        return Ok(());
    }
    let contents = read_to_string(cargo_path)
        .with_context(|| format!("Failed to read {}", cargo_path.display()))?;
    write(cargo_path, renamed_rust_package_source(&contents, name))
        .with_context(|| format!("Failed to update {}", cargo_path.display()))
}

pub(crate) fn renamed_rust_package_source(contents: &str, name: &str) -> String {
    let mut in_named_section = false;
    let mut updated = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" || trimmed == "[[bin]]" {
            in_named_section = true;
            updated.push(line.to_string());
            continue;
        }
        if in_named_section && trimmed.starts_with('[') {
            in_named_section = false;
        }
        if in_named_section && line.trim_start().starts_with("name = ") {
            updated.push(format!("name = \"{}\"", escape_toml_string(name)));
        } else {
            updated.push(line.to_string());
        }
    }
    updated.join("\n") + "\n"
}

fn rename_native_service_defaults(crate_dir: &Path, old: &str, new: &str) -> Result<()> {
    for relative in ["src/config/mod.rs", "tests/smoke.rs"] {
        let path = crate_dir.join(relative);
        if !path.exists() {
            continue;
        }
        let namespace = regex::Regex::new(&format!(
            r"\b{}::",
            regex::escape(&old.to_case(Case::Snake)),
        ))?;
        let source = namespace
            .replace_all(
                &read_to_string(&path)?,
                format!("{}::", new.to_case(Case::Snake)),
            )
            .into_owned()
            .replace(
                &format!("unwrap_or_else(|_| \"{old}\".to_string())"),
                &format!("unwrap_or_else(|_| \"{new}\".to_string())"),
            )
            .replace(
                &format!("name: \"{old}\".to_string()"),
                &format!("name: \"{new}\".to_string()"),
            )
            .replace(
                &format!("\"{}\".replace('_', \"-\")", old.to_case(Case::Snake)),
                &format!("\"{}\".replace('_', \"-\")", new.to_case(Case::Snake)),
            );
        write(path, source)?;
    }
    Ok(())
}

fn update_rust_root_cargo_metadata(
    app_root: &Path,
    author: Option<&str>,
    license: Option<&str>,
) -> Result<()> {
    if author.is_none() && license.is_none() {
        return Ok(());
    }

    let cargo_path = app_root.join("Cargo.toml");
    let existing = read_to_string(&cargo_path)
        .with_context(|| format!("Failed to read {}", cargo_path.display()))?;
    write(&cargo_path, cargo_metadata_source(&existing, author, license)?)
        .with_context(|| format!("Failed to update {}", cargo_path.display()))
}

/// The workspace manifest with new `[workspace.package]` authors/license, the
/// rest of the file (member order, comments, key order) as written.
fn cargo_metadata_source(existing: &str, author: Option<&str>, license: Option<&str>) -> Result<String> {
    let mut cargo = existing
        .parse::<toml_edit::DocumentMut>()
        .context("Failed to parse the root Cargo.toml")?;
    let workspace = cargo
        .entry("workspace")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("Root Cargo.toml workspace must be a TOML table")?;
    let package = workspace
        .entry("package")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("Root Cargo.toml workspace.package must be a TOML table")?;
    if let Some(author) = author {
        let mut authors = toml_edit::Array::new();
        if !author.trim().is_empty() {
            authors.push(author);
        }
        package["authors"] = toml_edit::value(authors);
    }
    if let Some(license) = license {
        package["license"] = toml_edit::value(license);
    }
    Ok(cargo.to_string())
}

fn update_rust_readme(app_root: &Path, previous_name: &str, target_name: &str, description: Option<&str>) -> Result<()> {
    let readme_path = app_root.join("README.md");
    if !readme_path.exists() {
        // The workspace README lives with the modules; only its generated
        // title follows a rename.
        let modules = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?
            .get("modules_path").and_then(Value::as_str).unwrap_or("src").to_string();
        let path = app_root.join(modules).join("README.md");
        if let Ok(existing) = read_to_string(&path)
            && let Some(rest) = existing.strip_prefix(&format!("# {previous_name}\n"))
            && previous_name != target_name
        {
            write(&path, format!("# {target_name}\n{rest}"))?;
        }
        return Ok(());
    }
    let existing = read_to_string(&readme_path)
        .with_context(|| format!("Failed to read {}", readme_path.display()))?;
    let current_description = existing
        .lines()
        .skip_while(|line| line.starts_with('#') || line.trim().is_empty())
        .next()
        .unwrap_or_default();
    write(
        &readme_path,
        format!(
            "# {target_name}\n\n{}\n",
            description.unwrap_or(current_description)
        ),
    )
    .with_context(|| format!("Failed to update {}", readme_path.display()))
}

fn ensure_service_shape(crate_dir: &Path, name: &str) -> Result<()> {
    if crate_dir.join("lib.rs").exists() {
        return native_layout::convert_shape(crate_dir, name, RustProjectKind::Service);
    }
    let crate_module = name.to_case(Case::Snake);
    create_dir_all(crate_dir.join("src/contracts"))?;
    create_dir_all(crate_dir.join("src/persistence"))?;
    let compliance = crate_dir.join("src/persistence/compliance_schema.rs");
    if !compliance.exists() {
        write(&compliance, "//! Conversion requires an explicit audit of existing owned storage.\npub fn declarations() -> Vec<forklaunch_core::persistence::compliance::ComplianceEntity> {\n    compile_error!(\"Declare the converted service owned storage before emitting compliance metadata\");\n}\n")?;
    }
    write(
        crate_dir.join("build.rs"),
        render_static_rust_template("rust/project/service/build.rs"),
    )?;
    write(
        crate_dir.join("src/contracts/mod.rs"),
        contracts_mod_rs(&["health".to_string()]),
    )?;
    write(
        crate_dir.join("src/contracts/health.rs"),
        render_static_rust_template("rust/project/service/src/contracts/health.rs"),
    )?;
    write(
        crate_dir.join("src/contracts/models.rs"),
        render_static_rust_template("rust/project/service/src/contracts/models.rs"),
    )?;
    let cargo_path = crate_dir.join("Cargo.toml");
    let manifest = read_to_string(&cargo_path)?;
    let mut manifest: toml::Value = toml::from_str(&manifest)?;
    let features = manifest["dependencies"]["forklaunch-core"].as_table_mut().unwrap()
        .entry("features").or_insert_with(|| toml::Value::Array(Vec::new())).as_array_mut().unwrap();
    if !features.iter().any(|feature| feature.as_str() == Some("typed-schema")) {
        features.push(toml::Value::String("typed-schema".into()));
    }
    let generated: toml::Value = toml::from_str(&service_cargo_toml(name, ""))?;
    manifest["build-dependencies"] = generated["build-dependencies"].clone();
    write(cargo_path, toml::to_string_pretty(&manifest)?)?;
    create_dir_all(crate_dir.join("src").join("routes"))?;
    create_dir_all(crate_dir.join("src").join("domain"))?;
    create_dir_all(crate_dir.join("src").join("persistence"))?;
    create_dir_all(crate_dir.join("src").join("config"))?;
    create_dir_all(crate_dir.join("src").join("error"))?;
    create_dir_all(crate_dir.join("tests"))?;
    write(crate_dir.join("src").join("lib.rs"), service_lib_rs(name))?;
    write(crate_dir.join("src").join("main.rs"), service_main_rs(name))?;
    write(
        crate_dir.join("src").join("routes").join("mod.rs"),
        routes_mod_rs(&rust_route_modules(&crate_dir.join("src").join("routes"))?),
    )?;
    write(
        crate_dir.join("src").join("routes").join("health.rs"),
        health_route_rs(),
    )?;
    write(
        crate_dir.join("src").join("domain").join("mod.rs"),
        service_domain_rs(),
    )?;
    write(
        crate_dir.join("src").join("error").join("mod.rs"),
        service_error_rs(),
    )?;
    write(
        crate_dir.join("src").join("persistence").join("mod.rs"),
        service_persistence_rs(),
    )?;
    write(
        crate_dir.join("src").join("config").join("mod.rs"),
        service_config_rs(name),
    )?;
    write(
        crate_dir.join("tests").join("smoke.rs"),
        service_smoke_test_rs(&crate_module),
    )?;
    Ok(())
}

fn ensure_worker_shape(crate_dir: &Path, name: &str) -> Result<()> {
    if crate_dir.join("lib.rs").exists() {
        return native_layout::convert_shape(crate_dir, name, RustProjectKind::Worker);
    }
    let crate_module = name.to_case(Case::Snake);
    create_dir_all(crate_dir.join("src").join("jobs"))?;
    create_dir_all(crate_dir.join("src").join("config"))?;
    create_dir_all(crate_dir.join("src").join("error"))?;
    create_dir_all(crate_dir.join("tests"))?;
    write(crate_dir.join("src").join("lib.rs"), worker_lib_rs(name))?;
    write(crate_dir.join("src").join("main.rs"), worker_main_rs(name))?;
    write(
        crate_dir.join("src").join("jobs").join("mod.rs"),
        worker_jobs_rs(),
    )?;
    write(
        crate_dir.join("src").join("error").join("mod.rs"),
        worker_error_rs(),
    )?;
    write(
        crate_dir.join("src").join("config").join("mod.rs"),
        worker_config_rs(name),
    )?;
    write(
        crate_dir.join("tests").join("smoke.rs"),
        worker_smoke_test_rs(&crate_module),
    )?;
    Ok(())
}

fn ensure_library_shape(crate_dir: &Path, name: &str) -> Result<()> {
    if crate_dir.join("lib.rs").exists() {
        return native_layout::convert_shape(crate_dir, name, RustProjectKind::Library);
    }
    let crate_module = name.to_case(Case::Snake);
    create_dir_all(crate_dir.join("src").join("error"))?;
    create_dir_all(crate_dir.join("src").join("models"))?;
    create_dir_all(crate_dir.join("src").join("service"))?;
    create_dir_all(crate_dir.join("tests"))?;
    write(crate_dir.join("src").join("lib.rs"), library_lib_rs(name))?;
    write(
        crate_dir.join("src").join("error").join("mod.rs"),
        library_error_rs(),
    )?;
    write(
        crate_dir.join("src").join("models").join("mod.rs"),
        library_models_rs(),
    )?;
    write(
        crate_dir.join("src").join("service").join("mod.rs"),
        library_service_rs(),
    )?;
    write(
        crate_dir.join("tests").join("smoke.rs"),
        library_smoke_test_rs(&crate_module),
    )?;
    Ok(())
}

fn rewrite_contracts_mod(directory: &Path) -> Result<()> {
    let mut modules = rust_route_modules(directory)?;
    modules.retain(|module| module != "models" && directory.join(format!("{module}.rs")).exists());
    write(directory.join("mod.rs"), contracts_mod_rs(&modules))?;
    Ok(())
}

fn rewrite_routes_mod(routes_dir: &Path, dryrun: bool, stdout: &mut StandardStream) -> Result<()> {
    let mut modules = rust_route_modules(routes_dir)?;
    let native = routes_dir.join("mod.rs").exists()
        && read_to_string(routes_dir.join("mod.rs"))?.contains("pub fn register_router(");
    if native {
        modules.retain(|module| routes_dir.join(format!("{module}.rs")).exists());
    }
    let content = if native {
        native_routes_mod_rs(&modules)
    } else {
        routes_mod_rs(&modules)
    };
    if let Some(project) = routes_dir.ancestors().find(|directory| directory.join("Cargo.toml").is_file()) {
        if shared_module_contracts(project) {
            rewrite_module_extensions(&project.join("api/contracts/extensions"), dryrun, stdout)?;
        }
    }
    write_text_file(&routes_dir.join("mod.rs"), &content, dryrun, stdout)
}

fn shared_module_contracts(project: &Path) -> bool {
    read_to_string(project.join("sdk.rs")).is_ok_and(|source| source.contains("pub use crate::api::contracts::registry::*"))
}

fn rewrite_module_extensions(directory: &Path, dryrun: bool, stdout: &mut StandardStream) -> Result<()> {
    let modules = rust_route_modules(directory)?.into_iter().filter(|module| directory.join(format!("{module}.rs")).is_file()).collect::<Vec<_>>();
    write_text_file(&directory.join("mod.rs"), &modules::extension_contracts_mod(&modules), dryrun, stdout)
}

fn rust_route_modules(routes_dir: &Path) -> Result<Vec<String>> {
    let mut modules = Vec::new();
    if routes_dir.exists() {
        for entry in std::fs::read_dir(routes_dir)
            .with_context(|| format!("Failed to read {}", routes_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            let Some(module) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if module != "mod" {
                modules.push(module.to_string());
            }
        }
    }
    if !modules.iter().any(|module| module == "health") {
        modules.push("health".to_string());
    }

    Ok(modules)
}

fn replace_router_in_manifest_value(
    manifest: &mut Value,
    project_name: &str,
    existing_name: &str,
    new_name: &str,
) -> Result<()> {
    let project = project_table_mut(manifest, project_name)
        .with_context(|| format!("Could not find Rust project {project_name} in manifest"))?;
    let routers = project
        .entry("routers")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("Project routers must be an array")?;
    let mut replaced = false;
    for router in routers.iter_mut() {
        if router.as_str() == Some(existing_name) {
            *router = Value::String(new_name.to_string());
            replaced = true;
        }
    }
    if !replaced
        && !routers
            .iter()
            .any(|router| router.as_str() == Some(new_name))
    {
        routers.push(Value::String(new_name.to_string()));
    }
    Ok(())
}

fn remove_router_from_manifest_value(
    manifest: &mut Value,
    project_name: &str,
    router_name: &str,
) -> Result<()> {
    let project = project_table_mut(manifest, project_name)
        .with_context(|| format!("Could not find Rust project {project_name} in manifest"))?;
    let routers = project
        .entry("routers")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("Project routers must be an array")?;
    routers.retain(|router| router.as_str() != Some(router_name));
    Ok(())
}

fn read_manifest_value(path: &Path) -> Result<Value> {
    let content =
        read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str::<Value>(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

fn write_manifest_value(path: &Path, value: &Value) -> Result<()> {
    let mut value = value.clone();
    crate::core::manifest::remove_retired_manifest_keys(&mut value);
    let mut content = toml::to_string_pretty(&value)?;
    if let Ok(original) = read_to_string(path) {
        content = crate::core::manifest::preserve_manifest_layout(&original, &content);
    }
    write(path, content)
        .with_context(|| format!("Failed to write {}", path.display()))
}

fn write_rust_rendered_templates(
    templates: &[RenderedTemplate],
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let mut writable_templates = Vec::new();
    for template in templates {
        if template.path.exists() && is_user_editable_rust_file(&template.path) {
            if dryrun {
                writeln!(stdout, "Would preserve {}", template.path.display())?;
            } else {
                writeln!(stdout, "Preserved {}", template.path.display())?;
            }
            continue;
        }
        let mut template = template.clone();
        template.content = dependencies::render_for_path(&template.path, &template.content)?;
        if !dryrun {
            template.content = native_layout::format_source(&template.path, &template.content);
        }
        writable_templates.push(template);
    }

    write_rendered_templates(&writable_templates, dryrun, stdout)
}

fn is_user_editable_rust_file(path: &Path) -> bool {
    if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
        return false;
    }

    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();

    if components
        .iter()
        .any(|component| *component == "tests" || *component == "__test__")
    {
        return true;
    }

    let Some(src_index) = components.iter().position(|component| *component == "src") else {
        return false;
    };
    if components.get(src_index + 2).is_some() && components[src_index + 1] != "lib.rs" {
        return true;
    }
    let Some(section) = components.get(src_index + 1).copied() else {
        return false;
    };

    match section {
        "lib.rs" | "main.rs" | "config" | "domain" | "persistence" | "error" | "jobs"
        | "models" | "service" => true,
        "routes" => true,
        _ => false,
    }
}

fn write_text_file(
    path: &Path,
    content: &str,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    if dryrun {
        writeln!(stdout, "Would write {}", path.display())?;
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory for {}", path.display()))?;
    }
    write(path, native_layout::format_source(path, content))
        .with_context(|| format!("Failed to write {}", path.display()))
}

fn write_text_file_preserving_existing(
    path: &Path,
    content: &str,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    if path.exists() && is_user_editable_rust_file(path) {
        if dryrun {
            writeln!(stdout, "Would preserve {}", path.display())?;
        } else {
            writeln!(stdout, "Preserved {}", path.display())?;
        }
        return Ok(());
    }

    write_text_file(path, content, dryrun, stdout)
}

fn escape_toml_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn escape_rust_string(value: &str) -> String {
    escape_toml_string(value)
}

#[cfg(test)]
mod workspace_tests {
    #[test]
    fn applications_without_a_selection_get_the_published_release() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".forklaunch")).unwrap();
        std::fs::write(root.path().join(".forklaunch/manifest.toml"), "modules_path = \"src\"\n").unwrap();
        let cargo_path = root.path().join("Cargo.toml");
        let original = "# application-owned workspace\n[workspace]\nmembers = []\n";
        std::fs::write(&cargo_path, original).unwrap();
        super::ensure_embedded_framework(root.path(), false).unwrap();
        assert_eq!(std::fs::read_to_string(&cargo_path).unwrap(), original);
        // Nothing is vendored: the published framework crates are selected.
        assert!(!root.path().join("vendor").exists());
        assert!(!root.path().join(".forklaunch/framework-rs").exists());
        assert!(super::dependencies::configured(root.path()));
    }

    use super::*;

    #[test]
    fn library_dependency_edits_keep_the_manifest_layout() {
        let source = "# mine\n[package]\nname = \"api\"\n\n[dependencies]\naxum = \"0.8\"\nold = { path = \"../core/old\" }\n";
        let mut cargo: Value = toml::from_str(source).unwrap();
        let dependencies = cargo["dependencies"].as_table_mut().unwrap();
        dependencies.remove("old");
        dependencies.insert("shared".into(), toml::from_str::<Value>("path = '../core/shared'").unwrap());
        assert_eq!(edited_dependencies(source, &cargo).unwrap(), "# mine\n[package]\nname = \"api\"\n\n[dependencies]\naxum = \"0.8\"\nshared = { path = \"../core/shared\" }\n");
        let unchanged: Value = toml::from_str(source).unwrap();
        assert_eq!(edited_dependencies(source, &unchanged).unwrap(), source);
    }

    #[test]
    fn application_metadata_keeps_the_workspace_manifest_layout() {
        let source = "[workspace]\nresolver = \"3\"\nmembers = [\n    \"src/b\",\n    \"src/a\",\n]\n\n[workspace.package]\nversion = \"0.1.0\"\nlicense = \"MIT\"\nauthors = [\"ForkLaunch\"]\n";
        let updated = cargo_metadata_source(source, Some("Jane Doe"), Some("Apache-2.0")).unwrap();
        assert_eq!(updated, source.replace("\"MIT\"", "\"Apache-2.0\"").replace("[\"ForkLaunch\"]", "[\"Jane Doe\"]"));
    }

    #[test]
    fn removing_custom_library_dependency_preserves_user_overrides() {
        let root = tempfile::tempdir().unwrap();
        let manifest: Value = toml::from_str(
            r#"
[[projects]]
name = "api"
type = "Service"
[projects.metadata]
language = "rust"
cratePath = "crates/api"
[[projects]]
name = "jobs"
type = "Worker"
[projects.metadata]
language = "rust"
cratePath = "crates/jobs"
"#,
        )
        .unwrap();
        for name in ["api", "jobs"] {
            create_dir_all(root.path().join(format!("crates/{name}"))).unwrap();
        }
        let api = root.path().join("crates/api/Cargo.toml");
        let jobs = root.path().join("crates/jobs/Cargo.toml");
        write(
            &api,
            r#"[dependencies]
models = { path = "../../shared/models" }
domain_models = { package = "models", path = "../../shared/models", features = ["custom"] }
external_models = { package = "models", path = "../../vendor/models" }
manual = { path = "../../shared/manual" }
"#,
        )
        .unwrap();
        let override_source = r#"# User override
[dependencies]
models = { path = "../../vendor/models" }
"#;
        write(&jobs, override_source).unwrap();
        remove_registered_library_dependency(root.path(), &manifest, "models", "shared/models")
            .unwrap();
        let cargo: Value = toml::from_str(&read_to_string(api).unwrap()).unwrap();
        assert!(cargo["dependencies"].get("models").is_none());
        assert!(cargo["dependencies"].get("domain_models").is_none());
        assert_eq!(cargo["dependencies"]["external_models"]["path"].as_str(), Some("../../vendor/models"));
        assert_eq!(
            cargo["dependencies"]["manual"]["path"].as_str(),
            Some("../../shared/manual")
        );
        assert_eq!(read_to_string(jobs).unwrap(), override_source);
    }

    #[test]
    fn native_sync_all_restores_custom_members_without_a_crates_directory() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        create_dir_all(root.path().join("shared/native-models/src")).unwrap();
        create_dir_all(root.path().join("modules/typescript-models")).unwrap();
        write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = []\n",
        )
        .unwrap();
        write(
            root.path().join("shared/native-models/Cargo.toml"),
            "[package]\nname = \"native-models\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        write(
            root.path().join("shared/native-models/src/lib.rs"),
            "pub const USER_OWNED: bool = true;\n",
        )
        .unwrap();
        write(
            root.path().join("modules/typescript-models/index.ts"),
            "export const userOwned = true;\n",
        )
        .unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            r#"language = "mixed"
app_name = "mixed"
runtime = "node"
[[projects]]
name = "native-models"
type = "Library"
[projects.metadata]
language = "rust"
cratePath = "shared/native-models"
[[projects]]
name = "typescript-models"
type = "Library"
[projects.metadata]
language = "typescript"
"#,
        )
        .unwrap();
        sync_all_rust_projects(root.path(), &mut StandardStream::stdout(ColorChoice::Never))
            .unwrap();
        let cargo: Value =
            toml::from_str(&read_to_string(root.path().join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(
            cargo["workspace"]["members"][0].as_str(),
            Some("shared/native-models")
        );
        assert!(!root.path().join("crates").exists());
        assert_eq!(
            read_to_string(root.path().join("shared/native-models/src/lib.rs")).unwrap(),
            "pub const USER_OWNED: bool = true;\n"
        );
        assert_eq!(
            read_to_string(root.path().join("modules/typescript-models/index.ts")).unwrap(),
            "export const userOwned = true;\n"
        );
        assert_eq!(
            registered_project_names(root.path(), RustProjectKind::Library)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn native_sync_cannot_remove_a_typescript_registration() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        let path = root.path().join(".forklaunch/manifest.toml");
        let source = r#"language = "mixed"
[[projects]]
name = "typescript-api"
type = "Service"
[projects.metadata]
language = "typescript"
[[projects]]
name = "native-jobs"
type = "Worker"
[projects.metadata]
language = "rust"
"#;
        write(&path, source).unwrap();
        let error =
            sync_rust_project(root.path(), RustProjectKind::Service, "typescript-api").unwrap_err();
        assert!(error.to_string().contains("refusing native sync cleanup"));
        assert_eq!(read_to_string(&path).unwrap(), source);
        assert_eq!(
            registered_project_names(root.path(), RustProjectKind::Service).unwrap(),
            HashSet::from(["typescript-api".into()])
        );
        assert_eq!(
            registered_project_names(root.path(), RustProjectKind::Worker).unwrap(),
            HashSet::from(["native-jobs".into()])
        );
    }

    #[test]
    fn cargo_members_support_compact_arrays_and_preserve_unrelated_metadata() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Cargo.toml");
        write(&path, "# user comment\n[workspace]\nmembers = [\"crates/a\"]\n[workspace.metadata]\nexample = \"crates/a\"\n").unwrap();
        add_cargo_member(root.path(), "crates/a", false).unwrap();
        add_cargo_member(root.path(), "crates/b", false).unwrap();
        replace_cargo_member(root.path(), "crates/a", "crates/c").unwrap();
        remove_cargo_member(root.path(), "crates/b").unwrap();
        let source = read_to_string(path).unwrap();
        let cargo: Value = toml::from_str(&source).unwrap();
        assert_eq!(
            cargo["workspace"]["members"].as_array().unwrap(),
            &[Value::String("crates/c".to_owned())]
        );
        assert_eq!(
            cargo["workspace"]["metadata"]["example"].as_str(),
            Some("crates/a")
        );
        assert!(source.contains("# user comment"));
    }
}

#[cfg(test)]
mod compliance_artifact_tests {
    use super::*;

    #[test]
    fn sync_moves_generated_declarations_beside_the_contract_host_only() {
        // Build scripts exactly as earlier releases generated them.
        const NATIVE_SERVICE: &str = "mod build_assets;\n#[path = \"persistence/compliance_schema.rs\"]\nmod compliance_schema;\nfn main() -> Result<(), Box<dyn std::error::Error>> {\n    println!(\"cargo:rerun-if-changed=persistence/compliance_schema.rs\");\n    println!(\"cargo:rerun-if-changed=compliance.json\");\n    let path = std::path::PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\")?).join(\"compliance.json\");\n    let metadata = forklaunch_core::persistence::compliance::compliance_manifest(&compliance_schema::declarations()).to_string();\n    if std::fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) {\n        std::fs::write(path, metadata)?;\n    }\n    println!(\"cargo:rerun-if-changed=domain/schemas\");\n    println!(\"cargo:rerun-if-changed=api/contracts\");\n    println!(\"cargo:rerun-if-changed=sdk.rs\");\n    println!(\"cargo:rerun-if-changed=build_assets.rs\");\n    let evidence = forklaunch_core::build::provenance::ServiceBuild::capture(&[])?;\n    let output = forklaunch_core::build::service_sdks(&contracts::registry(), \"{{{name}}}\", \"ForkLaunchClient\")?;\n    let evidence = evidence.include_assets(&build_assets::declared_assets(&output)?)?;\n    let evidence = if let Some(bindings) = build_assets::binding_declarations()? { evidence.with_bindings(bindings) } else { evidence };\n    build_assets::emit(&output)?;\n    evidence.record(\"{{{name}}}\", &output)?;\n    Ok(())\n}\n";
        const APALIS_WORKER: &str = "#[path = \"persistence/compliance_schema.rs\"]\nmod compliance_schema;\nmod build_assets;\nfn main() -> Result<(), Box<dyn std::error::Error>> {\n    println!(\"cargo:rerun-if-changed=persistence/compliance_schema.rs\");\n    println!(\"cargo:rerun-if-changed=compliance.json\");\n    println!(\"cargo:rerun-if-changed=build_assets.rs\");\n    let metadata = forklaunch_core::persistence::compliance::compliance_manifest(&compliance_schema::declarations()).to_string();\n    let path = std::path::Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"compliance.json\");\n    if std::fs::read_to_string(&path).ok().as_deref() != Some(metadata.as_str()) { std::fs::write(path, metadata)?; }\n    build_assets::generate()\n}\n";
        for (template, legacy) in [("native-service", NATIVE_SERVICE), ("apalis-worker", APALIS_WORKER)] {
            let root = tempfile::tempdir().unwrap();
            let legacy = legacy.to_owned();
            assert!(!legacy.contains("create_dir_all"), "{template}");
            write(root.path().join("build.rs"), &legacy).unwrap();
            write(root.path().join("compliance.json"), "{}").unwrap();
            migrate_compliance_artifact(root.path()).unwrap();
            let migrated = read_to_string(root.path().join("build.rs")).unwrap();
            assert_eq!(migrated.matches(".forklaunch/compliance.json").count(), 2, "{template}");
            assert!(migrated.contains("create_dir_all(path.parent()"), "{template}");
            syn::parse_file(&migrated).unwrap();
            assert!(!root.path().join("compliance.json").exists());

            let custom = legacy.replace(".join(\"compliance.json\")", ".join(\"audit/compliance.json\")");
            write(root.path().join("build.rs"), &custom).unwrap();
            migrate_compliance_artifact(root.path()).unwrap();
            assert_eq!(read_to_string(root.path().join("build.rs")).unwrap(), custom);
        }
    }
}

#[cfg(test)]
mod local_env_tests {
    use super::*;

    #[test]
    fn typescript_modules_of_mixed_apps_record_their_language() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        let manifest = "language = \"mixed\"\n\n[[projects]]\nname = \"ecommerce\"\nvariant = \"ecommerce-stripe\"\n\n[[projects]]\nname = \"billing\"\n\n[projects.metadata]\nlanguage = \"rust\"\n";
        write(root.path().join(".forklaunch/manifest.toml"), manifest).unwrap();
        record_typescript_language(root.path(), "ecommerce").unwrap();
        record_typescript_language(root.path(), "billing").unwrap();
        let value = read_manifest_value(&root.path().join(".forklaunch/manifest.toml")).unwrap();
        let language = |index: usize| value["projects"][index]["metadata"]["language"].as_str().map(str::to_owned);
        assert_eq!(language(0).as_deref(), Some("typescript"));
        assert_eq!(language(1).as_deref(), Some("rust"));
        // TypeScript-only applications are left as they are.
        let plain = "[[projects]]\nname = \"ecommerce\"\n";
        write(root.path().join(".forklaunch/manifest.toml"), plain).unwrap();
        record_typescript_language(root.path(), "ecommerce").unwrap();
        assert_eq!(read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap(), plain);
    }

    #[test]
    fn http_processes_get_their_required_settings_and_keep_existing_values() {
        let root = tempfile::tempdir().unwrap();
        let mut compose = DockerCompose::default();
        compose.services.insert("api".into(), rust_docker_service("app", "api", RustProjectKind::Service, Some(8004)));
        let worker = rust_docker_service("app", "jobs", RustProjectKind::Worker, None);
        assert!(!worker.environment.as_ref().unwrap().contains_key("MCP_ENABLED"));
        compose.services.insert("jobs".into(), worker);
        let mut producer = rust_docker_service("app", "jobs-server", RustProjectKind::Worker, None);
        producer.ports = Some(vec!["127.0.0.1:8005:3000".into()]);
        compose.services.insert("jobs-server".into(), producer);
        write(root.path().join("docker-compose.yaml"), yaml_to_string(&compose).unwrap()).unwrap();
        assert_eq!(compose.services["api"].environment.as_ref().unwrap()["MCP_ENABLED"], "true");

        let api = root.path().join("api");
        create_dir_all(&api).unwrap();
        write(api.join(".env.local"), "PORT=9999\nREDIS_URL=redis://localhost:6379").unwrap();
        ensure_rust_local_env(root.path(), &api, "api").unwrap();
        assert_eq!(
            read_to_string(api.join(".env.local")).unwrap(),
            "PORT=9999\nREDIS_URL=redis://localhost:6379\nHOST=127.0.0.1\nPROTOCOL=http\nVERSION=v1\nDOCS_PATH=/docs\nRUST_LOG=info\nMCP_ENABLED=true\n"
        );

        let jobs = root.path().join("jobs");
        create_dir_all(&jobs).unwrap();
        ensure_rust_local_env(root.path(), &jobs, "jobs").unwrap();
        assert!(read_to_string(jobs.join(".env.local")).unwrap().contains("PORT=8005\n"));

        let consumer = root.path().join("consumer");
        create_dir_all(&consumer).unwrap();
        ensure_rust_local_env(root.path(), &consumer, "consumer").unwrap();
        assert!(!consumer.join(".env.local").exists());
    }
}
