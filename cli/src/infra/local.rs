//! `forklaunch infra add|remove <service> <type>`: add or remove a capability
//! in a service's code.
//!
//! This is the documented way in for agents and people alike: it writes the
//! setup code (registrations.ts), the local stand-in (docker-compose), the
//! env files, the package dependency, the manifest resource and the test
//! utilities. It edits those files in place (see `in_place`): only the lines
//! the resource needs are added, in the file's own style, and `infra remove`
//! takes exactly those lines out again. (`change service --infrastructure`
//! regenerates the files instead, which reformats a customized service.)

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::Infrastructure,
    core::{command::command, manifest::ProjectType, validate::require_manifest},
};

/// What `infra add` can add: local infrastructure the service runs itself,
/// or a platform-held capability reached through the instance gateway.
enum Addable {
    Infrastructure,
    Capability(&'static super::capabilities::Capability),
}


/// Dependencies `infra add` ensures but `infra remove` leaves: every generated
/// TypeScript service declares ioredis whether or not it has a cache, so
/// removing it made an add/remove round trip drop a line that was there
/// before the add.
const CACHE_INDEPENDENT_DEPENDENCIES: &[&str] = &["ioredis"];

fn parse_addable(name: &str) -> Result<Addable> {
    if let Some(capability) = super::capabilities::find(name) {
        return Ok(Addable::Capability(capability));
    }
    parse_type(name)
        .map(|_| Addable::Infrastructure)
        .map_err(|_| {
            let capabilities: Vec<&str> = super::capabilities::CAPABILITIES
                .iter()
                .map(|c| c.id)
                .collect();
            anyhow::anyhow!(
                "unknown type '{name}'; supported: object-store (s3), cache (redis){}{}",
                if capabilities.is_empty() { "" } else { ", " },
                capabilities.join(", ")
            )
        })
}

/// Accepted type names: the infrastructure ids plus readable aliases.
fn parse_type(name: &str) -> Result<Infrastructure> {
    let id = match name {
        "object-store" | "objectstore" | "object_store" | "storage" => "s3",
        "cache" => "redis",
        other => other,
    };
    id.parse::<Infrastructure>().map_err(|_| {
        anyhow::anyhow!("unknown type '{name}'; supported: object-store (s3), cache (redis)")
    })
}

/// `--help` for the type: the infrastructure, then every managed gateway
/// capability (from `CAPABILITIES`, so a new one is listed by itself).
fn type_help() -> String {
    let capabilities = super::capabilities::CAPABILITIES;
    let ids: Vec<&str> = capabilities.iter().map(|c| c.id).collect();
    let on_workers: Vec<&str> = capabilities
        .iter()
        .filter(|c| c.workers)
        .map(|c| c.id)
        .collect();
    let workers = if on_workers.len() == ids.len() {
        "any of them also on a worker".to_string()
    } else {
        format!("{} also on a worker", on_workers.join(", "))
    };
    format!(
        "What to add: object-store (s3), cache (redis), or a managed gateway capability \
         (any ForkLaunch app, managed instance or not): {} ({workers})",
        ids.join(", ")
    )
}

/// Whether `infra add|remove` may change this capability on a project of
/// this type: services hold every capability; a worker those marked
/// `workers`; a library none.
fn check_project_type(
    name: &str,
    project_type: &ProjectType,
    capability: &super::capabilities::Capability,
) -> Result<()> {
    let allowed = match project_type {
        ProjectType::Service => true,
        ProjectType::Worker => capability.workers,
        ProjectType::Library => false,
    };
    if allowed {
        return Ok(());
    }
    let on_workers: Vec<&str> = super::capabilities::CAPABILITIES
        .iter()
        .filter(|c| c.workers)
        .map(|c| c.id)
        .collect();
    bail!(
        "'{name}' is not a service; {} is added to services (a worker can have: {})",
        capability.id,
        on_workers.join(", ")
    )
}

/// The note `infra add <worker> <capability>` prints when no HTTP service
/// holds a capability the platform sends events for: the worker can call the
/// gateway, but events are delivered over HTTP.
fn worker_events_note(
    projects: &[crate::core::manifest::ProjectEntry],
    worker: &str,
    capability: &super::capabilities::Capability,
    native: bool,
) -> Option<String> {
    if !capability.receives_events {
        return None;
    }
    let a_service_holds_it = projects.iter().any(|p| {
        p.name != worker
            && p.r#type == ProjectType::Service
            && p.resources
                .as_ref()
                .and_then(|r| r.capabilities.as_ref())
                .is_some_and(|c| c.iter().any(|c| c == capability.id))
    });
    if a_service_holds_it {
        return None;
    }
    let id = capability.id;
    let handler = if native {
        format!("capabilities/{id}.rs, served by its `-server` process")
    } else {
        format!("api/platformEvents/{id}.ts")
    };
    Some(format!(
        "note: '{worker}' can call the {id} gateway, but {id} events are delivered over HTTP \
         to /platform-events/{id}, never to the worker process. The handler is in its server \
         component ({handler}); if that is not reachable over HTTP where it is \
         deployed, also run `forklaunch infra add <service> {id}` on an HTTP service so the events \
         have somewhere to go."
    ))
}

fn args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("service").required(true).help(
            "The service (or, for a managed gateway capability, the worker) to change, by name",
        ),
    )
    .arg(Arg::new("type").required(true).help(type_help()))
    .arg(
        Arg::new("base_path")
            .short('p')
            .long("path")
            .help("Application root (defaults to the manifest in the current directory)"),
    )
    .arg(
        Arg::new("dryrun")
            .short('n')
            .long("dryrun")
            .help("Show what would change without writing")
            .action(ArgAction::SetTrue),
    )
}

/// Add or remove an object store or cache with minimal, in-place edits.
fn change_infrastructure(matches: &ArgMatches, adding: bool) -> Result<()> {
    use super::{capabilities::CapabilityEdit, in_place};
    use crate::core::{
        docker::{
            DockerCompose, add_redis_to_docker_compose, add_s3_to_docker_compose,
            clean_up_unused_infrastructure_services, remove_s3_from_docker_compose,
        },
        package_json::package_json_constants::{
            INFRASTRUCTURE_REDIS_VERSION, INFRASTRUCTURE_S3_VERSION, IOREDIS_VERSION,
        },
    };

    let service = matches.get_one::<String>("service").unwrap().clone();
    let wanted = parse_type(matches.get_one::<String>("type").unwrap())?;
    let (app_root, manifest) = require_manifest(matches)?;
    let project = manifest
        .projects
        .iter()
        .find(|p| p.name == service)
        .with_context(|| format!("no project named '{service}' in the manifest"))?;
    let native = project
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.language.as_deref())
        == Some("rust")
        || crate::init::rust_project::is_rust_project(&app_root, &service)?;
    // A Rust worker's processes (the worker and its HTTP server) share the
    // crate, so both get the cache or object store.
    let accepted =
        project.r#type == ProjectType::Service || (native && project.r#type == ProjectType::Worker);
    if !accepted {
        bail!(
            "'{service}' is not a service; `infra add` changes services and Rust workers (use `forklaunch change worker` for TypeScript workers)"
        );
    }
    let id = wanted.to_string();
    let resources = project.resources.as_ref();
    let (resource_key, present) = match wanted {
        Infrastructure::S3 => (
            "object_store",
            resources.and_then(|r| r.object_store.as_deref()) == Some(id.as_str()),
        ),
        Infrastructure::Redis => (
            "cache",
            resources.and_then(|r| r.cache.as_deref()) == Some(id.as_str()),
        ),
    };
    match (adding, present) {
        (true, true) => bail!("'{service}' already has {id}"),
        (false, false) => bail!("'{service}' does not have {id}"),
        _ => {}
    }

    if native {
        return super::native_local::change(matches, &app_root, &manifest, project, wanted, adding);
    }

    let app_name = manifest.app_name.clone();
    let service_path = app_root.join(&manifest.modules_path).join(&service);
    let mut edit = CapabilityEdit::new(
        &app_root,
        &app_name,
        &service,
        &service_path,
        Vec::new(),
        Vec::new(),
    );
    // Only files whose content actually changes are written.
    let stage = |edit: &mut CapabilityEdit, path: PathBuf, before: &str, after: String| {
        if after != before {
            edit.write(path, after);
        }
    };

    // registrations.ts: the env entries, the registration and its imports.
    let registrations = edit.registrations_path();
    let text = edit
        .read(&registrations)?
        .with_context(|| format!("{registrations:?} not found"))?;
    let changed = if adding {
        in_place::add_to_registrations(&text, &wanted, &app_name)?
    } else {
        in_place::remove_from_registrations(&text, &wanted, &app_name)?
    };
    if !adding
        && wanted == Infrastructure::Redis
        && in_place::still_referenced(&changed, "TtlCache")
    {
        eprintln!(
            "warning: registrations.ts still reads TtlCache (an auth or billing cache service, say); remove or replace those registrations"
        );
    }
    stage(&mut edit, registrations, &text, changed);

    // package.json: the dependency lines only.
    let dependencies: &[(&str, &str)] = match wanted {
        Infrastructure::S3 => &[("@forklaunch/infrastructure-s3", INFRASTRUCTURE_S3_VERSION)],
        Infrastructure::Redis => &[
            (
                "@forklaunch/infrastructure-redis",
                INFRASTRUCTURE_REDIS_VERSION,
            ),
            ("ioredis", IOREDIS_VERSION),
        ],
    };
    let package_json = service_path.join("package.json");
    if let Some(text) = edit.read(&package_json)? {
        let mut changed = text.clone();
        for (name, version) in dependencies {
            changed = if adding {
                in_place::add_dependency(&changed, name, version)?
            } else if CACHE_INDEPENDENT_DEPENDENCIES.contains(name) {
                changed
            } else {
                in_place::remove_dependency(&changed, name)?
            };
        }
        stage(&mut edit, package_json, &text, changed);
    }

    // .env.local: the local settings.
    let bucket = format!("{app_name}-{service}-dev");
    let env: Vec<(&str, &str)> = match wanted {
        Infrastructure::S3 => vec![
            ("S3_URL", "http://localhost:9000"),
            ("S3_BUCKET", bucket.as_str()),
            ("S3_REGION", "us-east-1"),
            ("S3_ACCESS_KEY_ID", "minioadmin"),
            ("S3_SECRET_ACCESS_KEY", "minioadmin"),
        ],
        Infrastructure::Redis => vec![("REDIS_URL", "redis://localhost:6379")],
    };
    if adding {
        edit.ensure_env_local(&env)?;
    } else {
        let env_local = service_path.join(".env.local");
        if let Some(text) = edit.read(&env_local)? {
            let keys: Vec<&str> = env.iter().map(|(k, _)| *k).collect();
            let changed = in_place::remove_env_lines(&text, &keys);
            if changed.trim().is_empty() {
                edit.delete(env_local);
            } else {
                stage(&mut edit, env_local, &text, changed);
            }
        }
    }

    // __test__/test-utils.ts: the test harness flag.
    let test_utils = service_path.join("__test__").join("test-utils.ts");
    if let Some(text) = edit.read(&test_utils)? {
        let flag = match wanted {
            Infrastructure::S3 => "needsS3",
            Infrastructure::Redis => "needsRedis",
        };
        let changed = in_place::set_test_harness_flag(&text, flag, adding)?;
        stage(&mut edit, test_utils, &text, changed);
    }

    // The manifest: the project's resource key only.
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let text = std::fs::read_to_string(&manifest_path)?;
    let changed = in_place::set_manifest_resource(
        &text,
        &service,
        resource_key,
        adding.then_some(id.as_str()),
    )?;
    stage(&mut edit, manifest_path, &text, changed);

    // docker-compose: the service's settings, and MinIO / Redis when missing
    // (an existing one is reused; on remove it goes once nothing uses it).
    let compose_path = app_root.join("docker-compose.yaml");
    if let Some(text) = edit.read(&compose_path)? {
        let mut compose: DockerCompose = serde_yml::from_str(&text)?;
        let before = serde_yml::to_string(&compose)?;
        // Serializing drops comments; keep the file's leading comment block.
        let header: String = text
            .split_inclusive('\n')
            .take_while(|l| l.starts_with('#') || l.trim().is_empty())
            .collect();
        if let Some(target) = compose.services.get(&service) {
            let mut environment = target.environment.clone().unwrap_or_default();
            match (&wanted, adding) {
                (Infrastructure::S3, true) => {
                    add_s3_to_docker_compose(&app_name, &service, &mut compose, &mut environment)?;
                }
                (Infrastructure::S3, false) => {
                    remove_s3_from_docker_compose(&mut compose, &mut environment)?;
                }
                (Infrastructure::Redis, true) => {
                    add_redis_to_docker_compose(&app_name, &mut compose, &mut environment, 0)?;
                }
                (Infrastructure::Redis, false) => {
                    environment.shift_remove("REDIS_URL");
                }
            }
            compose.services.get_mut(&service).unwrap().environment = Some(environment);
        } else {
            eprintln!(
                "warning: no '{service}' service in docker-compose.yaml; add its settings by hand"
            );
        }
        if !adding {
            let mut projects = manifest.projects.clone();
            for p in projects.iter_mut().filter(|p| p.name == service) {
                if let Some(r) = p.resources.as_mut() {
                    match wanted {
                        Infrastructure::S3 => r.object_store = None,
                        Infrastructure::Redis => r.cache = None,
                    }
                }
            }
            clean_up_unused_infrastructure_services(&mut compose, projects)?;
        }
        let after = serde_yml::to_string(&compose)?;
        if after != before {
            edit.write(compose_path, format!("{header}{after}"));
        }
    }

    if matches.get_flag("dryrun") {
        for (path, writes) in edit.changed_paths() {
            println!(
                "{} {}",
                if writes { "write " } else { "delete" },
                path.strip_prefix(&app_root).unwrap_or(&path).display()
            );
        }
        return Ok(());
    }
    edit.commit()?;
    println!(
        "{} {id} {} {service}",
        if adding { "Added" } else { "Removed" },
        if adding { "to" } else { "from" },
    );
    Ok(())
}

/// Add or remove a platform-held capability: the manifest's
/// `resources.capabilities`, the shared gateway wiring, and the capability's
/// own edits.
fn change_capability(
    matches: &ArgMatches,
    capability: &'static super::capabilities::Capability,
    adding: bool,
) -> Result<()> {
    use super::capabilities::CapabilityEdit;

    let service = matches.get_one::<String>("service").unwrap().clone();
    let (app_root, manifest) = require_manifest(matches)?;
    let project = manifest
        .projects
        .iter()
        .find(|p| p.name == service)
        .with_context(|| format!("no project named '{service}' in the manifest"))?;
    let is_worker = project.r#type == ProjectType::Worker;
    check_project_type(&service, &project.r#type, capability)?;
    let mut service_capabilities: Vec<String> = project
        .resources
        .as_ref()
        .and_then(|r| r.capabilities.clone())
        .unwrap_or_default();
    let present = service_capabilities.iter().any(|c| c == capability.id);
    match (adding, present) {
        (true, true) => bail!("'{service}' already has {}", capability.id),
        (false, false) => bail!("'{service}' does not have {}", capability.id),
        (true, false) => service_capabilities.push(capability.id.to_string()),
        (false, true) => service_capabilities.retain(|c| c != capability.id),
    }
    let app_capabilities: Vec<String> = manifest
        .projects
        .iter()
        .flat_map(|p| {
            if p.name == service {
                service_capabilities.clone()
            } else {
                p.resources
                    .as_ref()
                    .and_then(|r| r.capabilities.clone())
                    .unwrap_or_default()
            }
        })
        .collect();

    let native = project
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.language.as_deref())
        == Some("rust")
        || crate::init::rust_project::is_rust_project(&app_root, &service)?;
    let service_path = if native {
        let relative = project
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.crate_path.clone())
            .unwrap_or_else(|| format!("crates/{service}"));
        let relative = std::path::Path::new(&relative);
        if relative.components().any(|component| {
            matches!(component, std::path::Component::ParentDir
                | std::path::Component::RootDir | std::path::Component::Prefix(_))
        }) {
            bail!("Native capability crate path must remain within the application");
        }
        let relative: PathBuf = relative.components()
            .filter(|component| !matches!(component, std::path::Component::CurDir)).collect();
        let path = app_root.join(relative);
        if !path.canonicalize()?.starts_with(app_root.canonicalize()?) {
            bail!("Native capability crate path must remain within the application");
        }
        path
    } else {
        app_root.join(&manifest.modules_path).join(&service)
    };
    let mut edit = CapabilityEdit::new(
        &app_root,
        &manifest.app_name,
        &service,
        &service_path,
        service_capabilities.clone(),
        app_capabilities,
    );
    if is_worker {
        edit = edit.for_worker();
    }
    edit.native = native;
    edit.format_rust = native && adding;
    super::capabilities::apply(&mut edit, capability, adding)?;

    // The manifest: only the capability's entry in the service's
    // `resources.capabilities` changes (see `in_place`).
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let text = std::fs::read_to_string(&manifest_path)?;
    let changed = super::in_place::set_manifest_capability(&text, &service, capability.id, adding)?;
    if changed != text {
        edit.write(manifest_path, changed);
    }

    if matches.get_flag("dryrun") {
        for (path, writes) in edit.changed_paths() {
            println!(
                "{} {}",
                if writes { "write " } else { "delete" },
                path.strip_prefix(&app_root).unwrap_or(&path).display()
            );
        }
        return Ok(());
    }
    if native && adding {
        crate::init::rust_project::ensure_native_infrastructure_sources(&app_root)?;
    }
    edit.commit()?;
    if native {
        super::native_local::refresh_lockfile(&app_root);
    }
    println!(
        "{} {} {service}: {}",
        if adding { "Added" } else { "Removed" },
        capability.id,
        capability.summary
    );
    if adding && native {
        let factory = if capability.id == "models" {
            "models_client".to_owned()
        } else {
            format!("{}_client", capability.id)
        };
        let events = if capability.receives_events {
            format!("; its events reach capabilities/{}.rs", capability.id)
        } else {
            String::new()
        };
        println!("  Call crate::capabilities::{factory}() from the project's code{events}");
    }
    if adding && is_worker {
        if let Some(note) = worker_events_note(&manifest.projects, &service, capability, native) {
            eprintln!("{note}");
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct AddCommand;

impl CliCommand for AddCommand {
    fn command(&self) -> Command {
        args(command(
            "add",
            "Add a capability to a service (a managed gateway capability also to a worker): writes its setup code, local stand-in and manifest resource",
        ))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        if let Addable::Capability(capability) =
            parse_addable(matches.get_one::<String>("type").unwrap())?
        {
            return change_capability(matches, capability, true);
        }
        change_infrastructure(matches, true)
    }
}

#[derive(Debug)]
pub(super) struct RemoveCommand;

impl CliCommand for RemoveCommand {
    fn command(&self) -> Command {
        args(command(
            "remove",
            "Remove a capability from a service, undoing everything `infra add` wrote",
        ))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        if let Addable::Capability(capability) =
            parse_addable(matches.get_one::<String>("type").unwrap())?
        {
            return change_capability(matches, capability, false);
        }
        change_infrastructure(matches, false)
    }
}

#[cfg(test)]
mod tests {
    use super::{check_project_type, parse_type, type_help, worker_events_note};
    use crate::{
        constants::Infrastructure,
        core::manifest::{ProjectEntry, ProjectType},
        infra::capabilities::{CAPABILITIES, find},
    };

    #[test]
    fn workers_may_hold_every_capability_and_libraries_none() {
        for capability in CAPABILITIES {
            assert!(check_project_type("api", &ProjectType::Service, capability).is_ok());
            assert!(
                check_project_type("jobs", &ProjectType::Worker, capability).is_ok(),
                "{} refused on a worker",
                capability.id
            );
            let error = check_project_type("shared", &ProjectType::Library, capability)
                .unwrap_err()
                .to_string();
            assert!(error.contains("'shared' is not a service"), "{error}");
            assert!(error.contains("models"), "{error}");
        }
    }

    fn project(name: &str, project_type: &str, capabilities: &[&str]) -> ProjectEntry {
        let capabilities: Vec<String> = capabilities.iter().map(|c| format!("{c:?}")).collect();
        toml::from_str(&format!(
            "type = {project_type:?}\nname = {name:?}\ndescription = \"\"\n[resources]\ncapabilities = [{}]\n",
            capabilities.join(", ")
        ))
        .unwrap()
    }

    #[test]
    fn a_worker_only_event_capability_says_events_need_an_http_service() {
        let worker_only = vec![
            project("jobs", "Worker", &["sms", "models"]),
            project("api", "Service", &["email"]),
        ];
        let note = worker_events_note(&worker_only, "jobs", find("sms").unwrap(), false).unwrap();
        assert!(note.contains("delivered over HTTP"), "{note}");
        assert!(
            note.contains("forklaunch infra add <service> sms"),
            "{note}"
        );
        // Models have no events: nothing to say.
        assert!(worker_events_note(&worker_only, "jobs", find("models").unwrap(), false).is_none());

        let with_service = vec![
            project("jobs", "Worker", &["sms"]),
            project("api", "Service", &["sms"]),
        ];
        assert!(worker_events_note(&with_service, "jobs", find("sms").unwrap(), false).is_none());
    }

    #[test]
    fn help_names_every_capability_as_a_managed_gateway_one() {
        let help = type_help();
        assert!(help.contains("managed gateway capability"), "{help}");
        assert!(!help.contains("managed instance only"));
        for capability in CAPABILITIES {
            assert!(help.contains(capability.id), "{help}");
        }
        assert!(help.contains("any of them also on a worker"), "{help}");
    }

    #[test]
    fn aliases_resolve_to_infrastructure_ids() {
        assert_eq!(parse_type("object-store").unwrap(), Infrastructure::S3);
        assert_eq!(parse_type("s3").unwrap(), Infrastructure::S3);
        assert_eq!(parse_type("cache").unwrap(), Infrastructure::Redis);
        assert!(
            parse_type("email")
                .unwrap_err()
                .to_string()
                .contains("supported")
        );
    }
}

/// `infra add` / `infra remove` on a generated Rust application (a service
/// and a worker, see `tests/fixtures/rust_infra_app`).
#[cfg(test)]
mod rust_round_trip {
    use std::{collections::BTreeMap, fs, path::Path};

    use super::{AddCommand, RemoveCommand};
    use crate::{CliCommand, core::docker::DockerCompose};

    const CAPABILITIES: [&str; 8] = [
        "cache",
        "object-store",
        "email",
        "sms",
        "whatsapp",
        "voice",
        "payments",
        "models",
    ];

    fn fixture() -> tempfile::TempDir {
        fn copy(from: &Path, to: &Path) {
            fs::create_dir_all(to).unwrap();
            for entry in fs::read_dir(from).unwrap().flatten() {
                let target = to.join(entry.file_name());
                if entry.path().is_dir() {
                    copy(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let root = tempfile::tempdir().unwrap();
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_infra_app"),
            root.path(),
        );
        root
    }

    fn snapshot(root: &Path) -> BTreeMap<String, String> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let key = path.strip_prefix(root).unwrap().display().to_string();
                    out.insert(key, fs::read_to_string(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    fn run(command: &dyn CliCommand, root: &Path, project: &str, capability: &str) {
        let name = command.command().get_name().to_owned();
        let matches = command
            .command()
            .version("test")
            .try_get_matches_from([
                name.as_str(),
                project,
                capability,
                "--path",
                root.to_str().unwrap(),
            ])
            .unwrap();
        command
            .handler(&matches)
            .unwrap_or_else(|error| panic!("infra {name} {project} {capability}: {error:#}"));
    }

    fn compose(root: &Path) -> DockerCompose {
        serde_yml::from_str(&fs::read_to_string(root.join("docker-compose.yaml")).unwrap()).unwrap()
    }

    fn env(compose: &DockerCompose, service: &str, key: &str) -> Option<String> {
        compose.services[service]
            .environment
            .as_ref()?
            .get(key)
            .cloned()
    }

    /// Everything but `.env`, which keeps the field encryption key so values
    /// encrypted under it stay readable when a store is added again.
    fn without_kept_key(mut files: BTreeMap<String, String>) -> BTreeMap<String, String> {
        if let Some(env) = files.get_mut(".env") {
            *env = env
                .lines()
                .filter(|line| !line.starts_with("ENCRYPTION_KEY="))
                .map(|line| format!("{line}\n"))
                .collect();
        }
        files
    }

    #[test]
    fn each_capability_round_trips_on_a_rust_service_and_worker() {
        for project in ["notes", "jobs"] {
            for capability in CAPABILITIES {
                let root = fixture();
                let before = snapshot(root.path());
                run(&AddCommand, root.path(), project, capability);
                let added = snapshot(root.path());
                assert_ne!(added, before, "{project} {capability} changed nothing");
                run(&RemoveCommand, root.path(), project, capability);
                assert_eq!(
                    without_kept_key(snapshot(root.path())),
                    before,
                    "{project} {capability} did not round-trip"
                );
            }
        }
    }

    #[test]
    fn every_capability_together_wires_each_process_and_removes_in_any_order() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        for project in ["notes", "jobs"] {
            for capability in CAPABILITIES {
                run(&AddCommand, root, project, capability);
            }
        }
        let wired = compose(root);
        // Each project's Redis database is its own; a worker's two processes share one.
        assert_eq!(
            env(&wired, "notes", "REDIS_URL").as_deref(),
            Some("redis://redis:6379/0")
        );
        for process in ["jobs", "jobs-server"] {
            assert_eq!(
                env(&wired, process, "REDIS_URL").as_deref(),
                Some("redis://redis:6379/1")
            );
            assert_eq!(
                env(&wired, process, "S3_BUCKET").as_deref(),
                Some("demo-jobs-dev")
            );
            assert_eq!(
                env(&wired, process, "PLATFORM_GATEWAY_URL").as_deref(),
                Some("http://gateway-mock:8080")
            );
            let depends = wired.services[process].depends_on.as_ref().unwrap();
            for dependency in ["redis", "minio-init", "gateway-mock"] {
                assert!(
                    depends.contains_key(dependency),
                    "{process} -> {dependency}"
                );
            }
        }
        for process in ["notes", "jobs", "jobs-server"] {
            assert_eq!(
                env(&wired, process, "ENCRYPTION_KEY").as_deref(),
                Some(crate::infra::native_local::ENCRYPTION_KEY_REFERENCE)
            );
        }
        let buckets = crate::core::docker::minio_buckets(&wired);
        for bucket in ["forklaunch-telemetry", "demo-notes-dev", "demo-jobs-dev"] {
            assert!(
                buckets.iter().any(|b| b == bucket),
                "{bucket} in {buckets:?}"
            );
        }
        // Events reach the HTTP process: the newest holder, the worker's server.
        let mock = wired.services["gateway-mock"].environment.as_ref().unwrap();
        assert_eq!(mock["MOCK_EVENTS_URL_EMAIL"], "http://jobs-server:3000");
        assert!(!mock.contains_key("MOCK_EVENTS_URL_MODELS"));
        // Host runs see the same settings.
        let dotenv = fs::read_to_string(root.join(".env")).unwrap();
        let key = dotenv
            .lines()
            .find_map(|line| line.strip_prefix("ENCRYPTION_KEY="))
            .expect(".env holds the encryption key");
        for (crate_dir, database) in [("src/notes", 0), ("src/jobs", 1)] {
            let local = fs::read_to_string(root.join(crate_dir).join(".env.local")).unwrap();
            for line in [
                format!("REDIS_URL=redis://localhost:6379/{database}"),
                format!("ENCRYPTION_KEY={key}"),
                "S3_URL=http://localhost:9000".to_owned(),
                "PLATFORM_GATEWAY_URL=http://localhost:18088".to_owned(),
                "INSTANCE_HMAC_KEY=local-dev-instance-key".to_owned(),
            ] {
                assert!(local.lines().any(|l| l == line), "{crate_dir}: {line}");
            }
            let module =
                fs::read_to_string(root.join(crate_dir).join("capabilities/mod.rs")).unwrap();
            assert!(
                module.contains("pub fn models_client() -> BlueprintResult<ModelGatewayClient>")
            );
            assert!(module.contains("pub fn email_client()"));
        }
        for (project, order) in [
            (
                "jobs",
                [
                    "models",
                    "cache",
                    "email",
                    "object-store",
                    "payments",
                    "sms",
                    "voice",
                    "whatsapp",
                ],
            ),
            (
                "notes",
                [
                    "sms",
                    "object-store",
                    "whatsapp",
                    "models",
                    "cache",
                    "payments",
                    "voice",
                    "email",
                ],
            ),
        ] {
            for capability in order {
                run(&RemoveCommand, root, project, capability);
            }
        }
        assert_eq!(without_kept_key(snapshot(root)), before);
    }

    #[test]
    fn models_alone_writes_a_client_factory_and_no_event_route() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        let routes = fs::read_to_string(root.join("src/notes/api/routes/mod.rs")).unwrap();
        run(&AddCommand, root, "notes", "models");
        let module = fs::read_to_string(root.join("src/notes/capabilities/mod.rs")).unwrap();
        syn::parse_file(&module).unwrap();
        assert!(module.contains("use forklaunch_core::http::model_gateway::ModelGatewayClient;"));
        assert!(module.contains("Ok(ModelGatewayClient::new(instance_gateway()?))"));
        assert!(!module.contains("platform_events"), "{module}");
        assert!(!root.join("src/notes/capabilities/nonce_store.rs").exists());
        assert!(!root.join("src/notes/capabilities/models.rs").exists());
        assert!(
            !root
                .join("src/notes/api/controllers/platform_events.rs")
                .exists()
        );
        assert_eq!(
            fs::read_to_string(root.join("src/notes/api/routes/mod.rs")).unwrap(),
            routes
        );
        let cargo = fs::read_to_string(root.join("src/notes/Cargo.toml")).unwrap();
        assert!(cargo.contains("\"instance-gateway\""));
        let lib = fs::read_to_string(root.join("src/notes/lib.rs")).unwrap();
        assert!(
            lib.contains("pub mod bootstrapper;\npub mod capabilities;\npub mod config;"),
            "{lib}"
        );
        let wired = compose(root);
        assert_eq!(
            env(&wired, "notes", "FORKLAUNCH_LOCAL_GATEWAY_MOCK").as_deref(),
            Some("true")
        );
        assert!(wired.services.contains_key("gateway-mock"));
        // An event capability beside it mounts the route; removing it takes
        // the route back and keeps the models factory.
        run(&AddCommand, root, "notes", "email");
        assert!(
            fs::read_to_string(root.join("src/notes/api/routes/mod.rs"))
                .unwrap()
                .contains(".with_managed_capabilities()?")
        );
        run(&RemoveCommand, root, "notes", "email");
        assert_eq!(
            fs::read_to_string(root.join("src/notes/api/routes/mod.rs")).unwrap(),
            routes
        );
        assert_eq!(
            fs::read_to_string(root.join("src/notes/capabilities/mod.rs")).unwrap(),
            module
        );
        run(&RemoveCommand, root, "notes", "models");
        assert_eq!(snapshot(root), before);
    }
}
