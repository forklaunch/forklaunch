//! `forklaunch infra add|remove <service> <type>`: add or remove a capability
//! in a service's or a worker's code.
//!
//! This is the documented way in for agents and people alike: it writes the
//! setup code (registrations.ts), the local stand-in (docker-compose), the
//! env files, the package dependency, the manifest resource and the test
//! utilities. It edits those files in place (see `in_place`): only the lines
//! the resource needs are added, in the file's own style, and `infra remove`
//! takes exactly those lines out again. (`change service --infrastructure`
//! regenerates the files instead, which reformats a customized service.)
//!
//! The same holds for the files the application shares: the manifest keeps
//! every key the command does not set, and docker-compose.yaml is edited
//! line by line (see `compose_edit`) or, when that cannot be done safely,
//! left alone with the change printed.

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
        Arg::new("service")
            .required(true)
            .help("The service or worker to change, by name"),
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

/// `infra add <project> object-store` on a bucket that already exists.
fn existing_bucket_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("existing_bucket")
            .long("existing-bucket")
            .value_name("BUCKET")
            .help(
                "object-store only: declare the store on this bucket, which already exists, \
                 instead of one the platform creates. Needs --prefix. A platform that supports \
                 existing buckets then creates none and grants the project's task role those \
                 prefixes only; nothing deployed changes until then",
            ),
    )
    .arg(
        Arg::new("prefix")
            .long("prefix")
            .value_name("PREFIX")
            .action(ArgAction::Append)
            .help(
                "With --existing-bucket: a key prefix of the bucket this project may read and \
                 write (`uploads/`, `applications/*/logs/`). Repeat for several",
            ),
    )
}

/// An object store declared on a bucket that already exists: the bucket and
/// the key prefixes of it the project may use.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExistingBucket {
    pub(crate) bucket: String,
    pub(crate) prefixes: Vec<String>,
}

impl ExistingBucket {
    pub(crate) fn as_declaration(&self) -> (&str, &[String]) {
        (&self.bucket, &self.prefixes)
    }
}

/// S3's rules for a general purpose bucket name, the ones that keep a typo
/// (an ARN, a URL, a path) out of the manifest.
fn check_bucket_name(bucket: &str) -> Result<()> {
    let edge = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let valid = (3..=63).contains(&bucket.len())
        && bucket
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && edge(bucket.chars().next())
        && edge(bucket.chars().last())
        && !bucket.contains("..")
        // S3 refuses a name formatted as an IP address.
        && bucket.parse::<std::net::Ipv4Addr>().is_err();
    if !valid {
        bail!(
            "'{bucket}' is not a bucket name: give the bucket's name alone (3 to 63 lowercase \
             letters, digits, hyphens and dots), not an ARN, URL or path"
        );
    }
    Ok(())
}

/// A prefix is granted as `arn:aws:s3:::<bucket>/<prefix>*`, so it has to
/// name a part of the bucket: not all of it, and nothing IAM would expand.
fn check_prefix(prefix: &str) -> Result<()> {
    let reason = if prefix.is_empty() {
        "it is empty, which would grant the whole bucket"
    } else if prefix.starts_with('/') {
        "object keys do not start with '/'"
    } else if prefix.starts_with('*') || prefix.starts_with('?') {
        "it starts with a wildcard, which would grant the whole bucket"
    } else if prefix.contains("${") {
        "'${' would be read as an IAM policy variable"
    } else if prefix.chars().any(|c| c.is_whitespace() || c.is_control()) {
        "it contains whitespace"
    } else {
        return Ok(());
    };
    bail!("--prefix '{prefix}' cannot be used: {reason}")
}

/// The `--existing-bucket` / `--prefix` flags, checked. `None` when neither
/// was given (and for `infra remove`, which has neither).
fn existing_bucket(matches: &ArgMatches) -> Result<Option<ExistingBucket>> {
    let bucket = matches
        .try_get_one::<String>("existing_bucket")
        .ok()
        .flatten();
    let mut prefixes: Vec<String> = Vec::new();
    for prefix in matches
        .try_get_many::<String>("prefix")
        .ok()
        .flatten()
        .into_iter()
        .flatten()
    {
        if !prefixes.contains(prefix) {
            prefixes.push(prefix.clone());
        }
    }
    let Some(bucket) = bucket else {
        if !prefixes.is_empty() {
            bail!(
                "--prefix names a part of an existing bucket; give the bucket with --existing-bucket"
            );
        }
        return Ok(None);
    };
    check_bucket_name(bucket)?;
    if prefixes.is_empty() {
        bail!(
            "--existing-bucket needs at least one --prefix: the key prefix of '{bucket}' this \
             project may read and write (for example --prefix uploads/). Access is granted to \
             those prefixes only, never to the whole bucket"
        );
    }
    for prefix in &prefixes {
        check_prefix(prefix)?;
    }
    Ok(Some(ExistingBucket {
        bucket: bucket.clone(),
        prefixes,
    }))
}

/// What each variable the object-store or cache registration reads is for,
/// and who sets it once deployed.
fn environment_variable_note(key: &str) -> &'static str {
    match key {
        "S3_BUCKET" => "the bucket; the platform sets it for a bucket it provisions",
        "S3_REGION" => "the bucket's region; the platform sets it for a bucket it provisions",
        "S3_PREFIX" => {
            "optional key prefix every object is stored under; leave unset to use keys as written"
        }
        "S3_PRESIGN_MAX_UPLOAD_SECONDS" => {
            "longest presigned upload lifetime; the platform sets it for a bucket it provisions"
        }
        "S3_PRESIGN_MAX_DOWNLOAD_SECONDS" => {
            "longest presigned download lifetime; the platform sets it for a bucket it provisions"
        }
        "S3_URL" => "local MinIO endpoint only; unset when deployed",
        "S3_ACCESS_KEY_ID" | "S3_SECRET_ACCESS_KEY" => {
            "local MinIO key only; unset when deployed (the task role is used)"
        }
        "REDIS_URL" => "the cache's address; the platform sets it for a cache it provisions",
        _ => "read by the new registration",
    }
}

/// The deployed components that run a project's code, which are the scopes
/// its environment variables are stored under.
fn component_scopes(project: &str, project_type: &ProjectType) -> Vec<String> {
    match project_type {
        ProjectType::Worker => vec![format!("{project}-service"), format!("{project}-worker")],
        _ => vec![project.to_owned()],
    }
}

/// What `infra add` says about the variables it introduced: a production
/// release is held on a variable that is new to the code until someone
/// decides its value, so each one is named with what to do about it.
fn new_environment_report(keys: &[String], scopes: &[String]) -> String {
    let width = keys.iter().map(String::len).max().unwrap_or(0);
    let mut out = format!(
        "  New environment variables in registrations.ts ({}):\n",
        keys.len()
    );
    for key in keys {
        out.push_str(&format!(
            "    {key:width$}  {}\n",
            environment_variable_note(key)
        ));
    }
    out.push_str(&format!(
        "  Before a production release, set or explicitly unset each of them for {} in every \
         environment: a release that brings a variable nobody has decided is held.\n",
        match scopes {
            [only] => format!("the '{only}' scope"),
            scopes => format!(
                "the {} scopes",
                scopes
                    .iter()
                    .map(|scope| format!("'{scope}'"))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
        }
    ));
    out
}

/// The docker-compose services that run a TypeScript project's code. A
/// service is one. A worker is two: its HTTP server (`<name>-server` in a
/// generated stack; `<name>` or `<name>-service` in hand-written ones, the
/// first that exists) and its consumer (`<name>-worker`). A name that is
/// another project of the application is that project's, never the worker's.
fn typescript_compose_keys(
    compose: &crate::core::docker::DockerCompose,
    projects: &[crate::core::manifest::ProjectEntry],
    project: &str,
    project_type: &ProjectType,
) -> Vec<String> {
    let runs_here = |key: &String| {
        compose.services.contains_key(key)
            && (key == project || !projects.iter().any(|other| &other.name == key))
    };
    match project_type {
        ProjectType::Worker => {
            let server = [
                format!("{project}-server"),
                project.to_owned(),
                format!("{project}-service"),
            ]
            .into_iter()
            .find(runs_here);
            let consumer = Some(format!("{project}-worker")).filter(runs_here);
            server.into_iter().chain(consumer).collect()
        }
        _ => Some(project.to_owned())
            .filter(runs_here)
            .into_iter()
            .collect(),
    }
}

/// What to tell the user when docker-compose.yaml could not be edited in
/// place: the file is as it was, and this is the change to make by hand.
pub(crate) fn manual_compose_note(reason: &str, snippet: &str) -> String {
    let snippet: String = snippet
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect();
    format!(
        "note: docker-compose.yaml was left exactly as it is ({reason}). To match, make this \
         change by hand:\n{snippet}  Where a block is an alias today (`environment: *shared`), \
         keep what it brings in by making it a merge key (`<<: *shared`) above the new entries.\n"
    )
}

/// Add or remove an object store or cache with minimal, in-place edits.
fn change_infrastructure(matches: &ArgMatches, adding: bool) -> Result<()> {
    use super::{
        capabilities::CapabilityEdit,
        compose_edit::{self, ComposeRewrite},
        in_place,
    };
    use crate::core::{
        docker::{
            DockerCompose, add_redis_to_docker_compose, add_s3_to_docker_compose,
            clean_up_unused_infrastructure_services,
        },
        package_json::package_json_constants::{
            INFRASTRUCTURE_REDIS_VERSION, INFRASTRUCTURE_S3_VERSION, IOREDIS_VERSION,
        },
    };

    let service = matches.get_one::<String>("service").unwrap().clone();
    let wanted = parse_type(matches.get_one::<String>("type").unwrap())?;
    let existing = existing_bucket(matches)?;
    if existing.is_some() && wanted != Infrastructure::S3 {
        bail!(
            "--existing-bucket and --prefix describe an object store; they do not apply to a cache"
        );
    }
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
    // A worker's processes (the consumer and its HTTP server) share the
    // project's code, so both get what it gets. A TypeScript worker's
    // `cache` is its queue backend, which `change worker` owns.
    match (&project.r#type, &wanted) {
        (ProjectType::Service, _) | (ProjectType::Worker, Infrastructure::S3) => {}
        (ProjectType::Worker, Infrastructure::Redis) if native => {}
        (ProjectType::Worker, Infrastructure::Redis) => bail!(
            "'{service}' is a TypeScript worker: its cache is its queue backend, which `forklaunch change worker --type` sets. `infra add` gives a TypeScript worker an object store"
        ),
        (ProjectType::Library, _) => {
            bail!("'{service}' is a library; `infra add` changes services and workers")
        }
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
    let scopes = component_scopes(&service, &project.r#type);
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    match (adding, present) {
        // Declared already: the flags move the declaration onto the bucket.
        (true, true) if existing.is_some() => {
            let existing = existing.as_ref().unwrap();
            let text = std::fs::read_to_string(&manifest_path)?;
            let changed = in_place::set_manifest_object_store(
                &text,
                &service,
                Some(id.as_str()),
                Some(existing.as_declaration()),
            )?;
            if matches.get_flag("dryrun") {
                if changed != text {
                    println!("write  .forklaunch/manifest.toml");
                }
                return Ok(());
            }
            if changed != text {
                std::fs::write(&manifest_path, changed)?;
            }
            println!("Updated {id} on {service}");
            print!("{}", existing_bucket_report(existing, &scopes));
            return Ok(());
        }
        (true, true) => bail!(
            "'{service}' already has {id}{}",
            if wanted == Infrastructure::S3 {
                " (to point it at an existing bucket, run this again with --existing-bucket and --prefix)"
            } else {
                ""
            }
        ),
        (false, false) => bail!("'{service}' does not have {id}"),
        _ => {}
    }

    if native {
        return super::native_local::change(
            matches,
            &app_root,
            &manifest,
            project,
            wanted,
            adding,
            existing.as_ref(),
        );
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

    let registrations = edit.registrations_path();
    let registrations_text = edit
        .read(&registrations)?
        .with_context(|| format!("{registrations:?} not found"))?;
    // A service that registers its own ObjectStore is declared, not rewired:
    // its registration, the variables it reads, its tests and its local
    // stack are its own. `infra remove` likewise takes out only the
    // registration `infra add` wrote.
    let own_registration = wanted == Infrastructure::S3
        && in_place::registers(&registrations_text, &wanted)
        && (adding || !in_place::registration_is_generated(&registrations_text, &wanted));
    let new_environment = if adding && !own_registration {
        in_place::new_environment_keys(&registrations_text, &wanted)?
    } else {
        Vec::new()
    };
    let mut manual_compose = None;

    if !own_registration {
        // registrations.ts: the env entries, the registration and its imports.
        let changed = if adding {
            in_place::add_to_registrations(&registrations_text, &wanted, &app_name)?
        } else {
            in_place::remove_from_registrations(&registrations_text, &wanted, &app_name)?
        };
        if !adding
            && wanted == Infrastructure::Redis
            && in_place::still_referenced(&changed, "TtlCache")
        {
            eprintln!(
                "warning: registrations.ts still reads TtlCache (an auth or billing cache service, say); remove or replace those registrations"
            );
        }
        stage(&mut edit, registrations, &registrations_text, changed);

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
                // Only the lines still holding what `infra add` wrote: a
                // value the developer set is theirs.
                let changed = in_place::remove_env_pairs(&text, &env);
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
    }

    // The manifest: the project's resource keys only.
    let text = std::fs::read_to_string(&manifest_path)?;
    let changed = match wanted {
        Infrastructure::S3 => in_place::set_manifest_object_store(
            &text,
            &service,
            adding.then_some(id.as_str()),
            existing.as_ref().map(ExistingBucket::as_declaration),
        )?,
        Infrastructure::Redis => in_place::set_manifest_resource(
            &text,
            &service,
            resource_key,
            adding.then_some(id.as_str()),
        )?,
    };
    stage(&mut edit, manifest_path, &text, changed);

    // docker-compose: the settings of the project's processes, and MinIO /
    // Redis when missing (an existing one is reused; on remove it goes once
    // nothing uses it). A setting a process already has, its own or merged in
    // (`<<: *shared`), is the application's and stays.
    let compose_path = app_root.join("docker-compose.yaml");
    if !own_registration && let Some(text) = edit.read(&compose_path)? {
        let mut compose: DockerCompose = serde_yml::from_str(&text)?;
        let keys = typescript_compose_keys(&compose, &manifest.projects, &service, &project.r#type);
        if keys.is_empty() {
            eprintln!(
                "warning: no '{service}' service in docker-compose.yaml; add its settings by hand"
            );
        }
        // What `infra add` writes for the backend, to recognize it on remove.
        let mut generated_stack = DockerCompose::default();
        match (&wanted, adding) {
            (Infrastructure::S3, true) if !keys.is_empty() => {
                let mut generated = indexmap::IndexMap::new();
                add_s3_to_docker_compose(&app_name, &service, &mut compose, &mut generated)?;
                // Each process gets its own lines, also when one merges in
                // another's block: what a process inherits is not this
                // command's to rely on (see `compose_edit::rewrite`).
                for key in &keys {
                    let set = compose_edit::effective_environment(&text, key);
                    let environment = compose
                        .services
                        .get_mut(key)
                        .expect("a compose key of this project")
                        .environment
                        .get_or_insert_with(Default::default);
                    for (name, value) in &generated {
                        if !set.contains(name) && !environment.contains_key(name) {
                            environment.insert(name.clone(), value.clone());
                        }
                    }
                }
            }
            (Infrastructure::S3, false) => {
                let mut generated = indexmap::IndexMap::new();
                add_s3_to_docker_compose(
                    &app_name,
                    &service,
                    &mut generated_stack,
                    &mut generated,
                )?;
                for key in &keys {
                    let Some(environment) = compose
                        .services
                        .get_mut(key)
                        .and_then(|process| process.environment.as_mut())
                    else {
                        continue;
                    };
                    for (name, value) in &generated {
                        match environment.get(name) {
                            Some(current) if current == value => {
                                environment.shift_remove(name);
                            }
                            Some(_) => {
                                edit.note_kept_setting(&compose_path, &format!("{key} {name}"))
                            }
                            None => {}
                        }
                    }
                }
            }
            (Infrastructure::Redis, _) => {
                for key in &keys {
                    let mut environment = compose.services[key]
                        .environment
                        .clone()
                        .unwrap_or_default();
                    if adding {
                        add_redis_to_docker_compose(&app_name, &mut compose, &mut environment, 0)?;
                    } else {
                        environment.shift_remove("REDIS_URL");
                    }
                    compose.services.get_mut(key).unwrap().environment = Some(environment);
                }
            }
            _ => {}
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
            // The model holds a merge key as the text it stood for when the
            // file was read; read the file as edited so far, so a setting
            // just removed from a shared block no longer counts as a use.
            if let ComposeRewrite::Edited(updated) = compose_edit::rewrite(&text, &compose)? {
                compose = serde_yml::from_str(&updated)?;
            }
            let backends: Vec<String> = generated_stack.services.keys().cloned().collect();
            clean_up_unused_infrastructure_services(&mut compose, projects)?;
            // The backend's volume goes with it when it is still the one
            // `infra add` wrote and nothing mounts it, a mount merged in
            // through an anchor included. When that cannot be told, it stays.
            if backends
                .iter()
                .any(|backend| !compose.services.contains_key(backend))
            {
                let mounted = match compose_edit::rewrite(&text, &compose)? {
                    ComposeRewrite::Edited(updated) => compose_edit::mounted_volumes(&updated),
                    _ => None,
                };
                for (name, generated) in &generated_stack.volumes {
                    let ours = compose
                        .volumes
                        .get(name)
                        .map(serde_json::to_value)
                        .transpose()?
                        == Some(serde_json::to_value(generated)?);
                    let unmounted = mounted
                        .as_ref()
                        .is_some_and(|mounted| !mounted.contains(name));
                    if ours && unmounted {
                        compose.volumes.shift_remove(name);
                    }
                }
            }
        }
        match compose_edit::rewrite(&text, &compose)? {
            ComposeRewrite::Unchanged => {}
            ComposeRewrite::Edited(updated) => edit.write(compose_path, updated),
            ComposeRewrite::Manual { reason, snippet } => {
                manual_compose = Some(manual_compose_note(&reason, &snippet));
            }
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
        if !new_environment.is_empty() {
            print!("{}", new_environment_report(&new_environment, &scopes));
        }
        if let Some(note) = manual_compose {
            eprint!("{note}");
        }
        return Ok(());
    }
    edit.commit()?;
    println!(
        "{} {id} {} {service}",
        if adding { "Added" } else { "Removed" },
        if adding { "to" } else { "from" },
    );
    if own_registration {
        println!("{}", own_registration_report(adding));
    }
    if let Some(existing) = &existing {
        print!("{}", existing_bucket_report(existing, &scopes));
    }
    if !new_environment.is_empty() {
        print!("{}", new_environment_report(&new_environment, &scopes));
    }
    if let Some(note) = manual_compose {
        eprint!("{note}");
    }
    Ok(())
}

/// What `infra add|remove` says when the ObjectStore registration is not one
/// it wrote: the declaration changed and the code did not.
fn own_registration_report(adding: bool) -> &'static str {
    if adding {
        "  registrations.ts already registers an ObjectStore of its own, so only the manifest \
         declaration was added: the registration, its environment variables, tests, env files \
         and docker-compose.yaml are as they were, and no new environment variables were \
         introduced."
    } else {
        "  The ObjectStore registration in registrations.ts is not the one `infra add` writes, \
         so only the manifest declaration was removed: the registration, its environment \
         variables, dependency, tests, env files and docker-compose.yaml are as they were. \
         Take them out by hand if the service no longer stores files."
    }
}

/// What an existing-bucket declaration means once deployed. It only records
/// what the platform should do, so it says what to keep until a deploy shows
/// the platform did it.
pub(crate) fn existing_bucket_report(existing: &ExistingBucket, scopes: &[String]) -> String {
    let quoted = |items: &[String], joiner: &str| {
        items
            .iter()
            .map(|item| format!("'{item}'"))
            .collect::<Vec<_>>()
            .join(joiner)
    };
    format!(
        "  Recorded: the object store is the existing bucket '{}', limited to {}.\n  \
         A platform that supports existing buckets creates no bucket for it and grants {} read \
         and write access to those prefixes only, through the task role. This command changes \
         nothing that is deployed: keep the bucket settings and credentials the environment \
         has (S3_BUCKET and any stored S3 keys) until a deploy shows that grant in place.\n",
        existing.bucket,
        quoted(&existing.prefixes, ", "),
        quoted(scopes, " and "),
    )
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
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            bail!("Native capability crate path must remain within the application");
        }
        let relative: PathBuf = relative
            .components()
            .filter(|component| !matches!(component, std::path::Component::CurDir))
            .collect();
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
        existing_bucket_args(args(command(
            "add",
            "Add a capability to a service or worker (the cache only to a service or a Rust worker): writes its setup code, local stand-in and manifest resource",
        )))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        if let Addable::Capability(capability) =
            parse_addable(matches.get_one::<String>("type").unwrap())?
        {
            if existing_bucket(matches)?.is_some() {
                bail!(
                    "--existing-bucket and --prefix describe an object store; they do not apply to {}",
                    capability.id
                );
            }
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
    use super::{
        ExistingBucket, check_project_type, component_scopes, existing_bucket_report,
        manual_compose_note, new_environment_report, own_registration_report, parse_type,
        type_help, worker_events_note,
    };
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
    fn new_variables_are_listed_with_the_release_gate_and_the_scopes_they_need() {
        let keys = vec!["S3_BUCKET".to_string(), "S3_PREFIX".to_string()];
        let service =
            new_environment_report(&keys, &component_scopes("api", &ProjectType::Service));
        assert!(
            service.contains("New environment variables in registrations.ts (2)"),
            "{service}"
        );
        assert!(service.contains("    S3_BUCKET  the bucket;"), "{service}");
        assert!(
            service.contains("    S3_PREFIX  optional key prefix"),
            "{service}"
        );
        assert!(
            service.contains(
                "set or explicitly unset each of them for the 'api' scope in every environment"
            ),
            "{service}"
        );
        assert!(service.contains("production release"), "{service}");
        // A worker is two deployed components, so two scopes.
        let worker = new_environment_report(&keys, &component_scopes("jobs", &ProjectType::Worker));
        assert!(
            worker.contains("for the 'jobs-service' and 'jobs-worker' scopes"),
            "{worker}"
        );
    }

    #[test]
    fn an_existing_bucket_is_described_by_what_the_platform_will_do() {
        let existing = ExistingBucket {
            bucket: "shared".to_string(),
            prefixes: vec!["a/".to_string(), "b/".to_string()],
        };
        let report =
            existing_bucket_report(&existing, &component_scopes("jobs", &ProjectType::Worker));
        assert!(
            report.contains("existing bucket 'shared', limited to 'a/', 'b/'"),
            "{report}"
        );
        assert!(
            report.contains("'jobs-service' and 'jobs-worker'"),
            "{report}"
        );
        // It records a declaration; it does not claim access is in place.
        assert!(
            report.contains("A platform that supports existing buckets"),
            "{report}"
        );
        assert!(
            report.contains("keep the bucket settings and credentials"),
            "{report}"
        );
    }

    #[test]
    fn a_registration_of_the_services_own_is_reported_as_left_alone() {
        let added = own_registration_report(true);
        assert!(
            added.contains("only the manifest declaration was added"),
            "{added}"
        );
        assert!(added.contains("no new environment variables"), "{added}");
        let removed = own_registration_report(false);
        assert!(
            removed.contains("only the manifest declaration was removed"),
            "{removed}"
        );
        assert!(removed.contains("by hand"), "{removed}");
    }

    #[test]
    fn a_compose_file_left_alone_comes_with_the_change_to_make() {
        let note = manual_compose_note(
            "`services.api.environment` is not a block mapping",
            "services:\n  api:\n    environment:\n      S3_URL: http://minio:9000\n",
        );
        assert!(
            note.contains(
                "left exactly as it is (`services.api.environment` is not a block mapping)"
            ),
            "{note}"
        );
        assert!(note.contains("    services:\n      api:\n        environment:\n          S3_URL: http://minio:9000\n"), "{note}");
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

/// `infra add` / `infra remove` on a hand-customized TypeScript application
/// (`tests/fixtures/ts_infra_app`): a service, a worker and a service with
/// its own object-store registration; a compose file with comments, anchors
/// and merge keys; a manifest with a retired key and keys no CLI knows.
#[cfg(test)]
mod typescript_fixture {
    use std::{collections::BTreeMap, fs, path::Path};

    use super::{AddCommand, RemoveCommand};
    use crate::{CliCommand, core::manifest::application::ApplicationManifestData};

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
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_infra_app"),
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

    fn try_run(command: &dyn CliCommand, root: &Path, args: &[&str]) -> anyhow::Result<()> {
        let name = command.command().get_name().to_owned();
        let mut argv = vec![name.as_str()];
        argv.extend(args);
        argv.extend(["--path", root.to_str().unwrap()]);
        let matches = command
            .command()
            .version("test")
            .try_get_matches_from(argv)?;
        command.handler(&matches)
    }

    fn run(command: &dyn CliCommand, root: &Path, args: &[&str]) {
        try_run(command, root, args).unwrap_or_else(|error| panic!("infra {args:?}: {error:#}"));
    }

    /// The paths whose content differs between two snapshots.
    fn changed(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<String> {
        let mut paths: Vec<String> = before
            .keys()
            .chain(after.keys())
            .filter(|path| before.get(*path) != after.get(*path))
            .cloned()
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Lines of `before` that `after` no longer has, and the ones it gained.
    fn line_changes(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
        let mut gained: Vec<&str> = after.lines().collect();
        let mut lost = Vec::new();
        for line in before.lines() {
            match gained.iter().position(|l| *l == line) {
                Some(index) => {
                    gained.remove(index);
                }
                None => lost.push(line.to_owned()),
            }
        }
        (lost, gained.into_iter().map(str::to_owned).collect())
    }

    const MANIFEST: &str = ".forklaunch/manifest.toml";
    const COMPOSE: &str = "docker-compose.yaml";

    #[test]
    fn an_object_store_goes_on_a_service_and_a_typescript_worker_and_comes_off_again() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        run(&AddCommand, root, &["api", "object-store"]);
        run(&AddCommand, root, &["jobs", "object-store"]);
        let added = snapshot(root);

        // The manifest: two lines gained, nothing lost. The retired key, the
        // unknown keys and the comments are where they were.
        let (lost, gained) = line_changes(&before[MANIFEST], &added[MANIFEST]);
        assert_eq!(lost, Vec::<String>::new());
        assert_eq!(gained, ["object_store = \"s3\"", "object_store = \"s3\""]);
        for kept in [
            "snake_case_name = \"jobs\"",
            "future_application_setting = { keep = true }",
            "future_project_setting = \"kept\"",
            "cache = \"redis\" # also the session store",
            "hostingType = \"ecs-ec2\"",
        ] {
            assert!(added[MANIFEST].lines().any(|line| line == kept), "{kept}");
        }
        let manifest: ApplicationManifestData = toml::from_str(&added[MANIFEST]).unwrap();
        for project in ["api", "jobs"] {
            let resources = manifest
                .projects
                .iter()
                .find(|p| p.name == project)
                .and_then(|p| p.resources.as_ref())
                .unwrap();
            assert_eq!(resources.object_store.as_deref(), Some("s3"), "{project}");
            assert!(resources.object_store_bucket.is_none());
        }

        // docker-compose.yaml: lines gained only. Comments, the anchors, the
        // merge keys, the flow lists and the block scalar are untouched.
        let (lost, gained) = line_changes(&before[COMPOSE], &added[COMPOSE]);
        assert_eq!(lost, Vec::<String>::new(), "{}", added[COMPOSE]);
        let compose = &added[COMPOSE];
        // The API's settings follow its own last one.
        assert!(
            compose.contains(
                "      REDIS_URL: redis://redis:6379/0\n      S3_URL: http://minio:9000\n      S3_BUCKET: demo-api-dev\n      S3_REGION: us-east-1\n      S3_ACCESS_KEY_ID: minioadmin\n      S3_SECRET_ACCESS_KEY: minioadmin\n    depends_on:\n"
            ),
            "{compose}"
        );
        // The worker's two containers share one anchored block. Each still
        // gets its own lines: nothing is left to what a merge key brings in.
        assert!(
            compose.contains(
                "      REDIS_URL: redis://redis:6379/1\n      S3_URL: http://minio:9000\n      S3_BUCKET: demo-jobs-dev\n"
            ),
            "{compose}"
        );
        assert!(
            compose.contains(
                "      <<: *jobs-environment\n      WORKER_CONCURRENCY: \"4\"\n      S3_URL: http://minio:9000\n      S3_BUCKET: demo-jobs-dev\n"
            ),
            "{compose}"
        );
        assert_eq!(gained.iter().filter(|l| l.contains("S3_BUCKET")).count(), 3);
        // The service with storage of its own keeps its endpoint and bucket.
        assert!(compose.contains(
            "      S3_URL: http://host.docker.internal:9100\n      S3_BUCKET: demo-shared-files\n"
        ));
        // MinIO and its volume are appended; the file still parses as YAML
        // with its aliases resolved.
        assert!(
            compose.contains("  redis-data:\n  minio-data:\n    driver: local\n"),
            "{compose}"
        );
        assert!(
            compose.contains("\n  minio:\n    container_name: demo-minio\n"),
            "{compose}"
        );
        serde_yml::from_str::<serde_yml::Value>(compose).unwrap();

        // The worker's code is wired like the service's.
        for project in ["api", "jobs"] {
            let module = format!("src/modules/{project}");
            let registrations = &added[&format!("{module}/registrations.ts")];
            assert!(
                registrations.contains("  ObjectStore: {\n"),
                "{registrations}"
            );
            assert!(registrations.contains("    type: S3ObjectStore,\n"));
            assert!(registrations.contains("getEnvVar('S3_PRESIGN_MAX_UPLOAD_SECONDS')"));
            assert!(
                registrations.contains("from '@forklaunch/infrastructure-s3';"),
                "{registrations}"
            );
            assert!(
                added[&format!("{module}/package.json")]
                    .contains("\"@forklaunch/infrastructure-s3\": ")
            );
            assert!(added[&format!("{module}/__test__/test-utils.ts")].contains("needsS3: true"));
            assert!(
                added[&format!("{module}/.env.local")]
                    .contains(&format!("S3_BUCKET=demo-{project}-dev\n"))
            );
        }
        // The service with storage of its own was not touched.
        assert!(
            changed(&before, &added)
                .iter()
                .all(|path| !path.contains("modules/files"))
        );

        // And every byte comes back.
        run(&RemoveCommand, root, &["jobs", "object-store"]);
        run(&RemoveCommand, root, &["api", "object-store"]);
        assert_eq!(snapshot(root), before);
    }

    #[test]
    fn an_existing_bucket_is_declared_without_touching_a_service_that_registers_its_own_store() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        run(
            &AddCommand,
            root,
            &[
                "files",
                "object-store",
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                "tenants/",
                "--prefix",
                "exports/*/daily/",
            ],
        );
        let added = snapshot(root);
        // Only the manifest changed: no new environment variables, no test
        // flag, no local settings, no compose edit.
        assert_eq!(changed(&before, &added), [MANIFEST]);
        let (lost, gained) = line_changes(&before[MANIFEST], &added[MANIFEST]);
        assert_eq!(lost, Vec::<String>::new());
        assert_eq!(
            gained,
            [
                "object_store = \"s3\"",
                "object_store_bucket = \"demo-shared-files\"",
                "object_store_prefixes = [\"tenants/\", \"exports/*/daily/\"]",
            ]
        );
        assert!(added[MANIFEST].contains(
            "[projects.resources]\ndatabase = \"postgresql\"\nobject_store = \"s3\"\nobject_store_bucket = \"demo-shared-files\"\n"
        ));

        // Running it again with other prefixes moves the declaration.
        run(
            &AddCommand,
            root,
            &[
                "files",
                "object-store",
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                "tenants/",
            ],
        );
        let moved = snapshot(root);
        assert_eq!(changed(&added, &moved), [MANIFEST]);
        assert!(moved[MANIFEST].contains("object_store_prefixes = [\"tenants/\"]\n"));
        // Without the flags a second add is refused, and says how.
        let error = try_run(&AddCommand, root, &["files", "object-store"])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("already has s3") && error.contains("--existing-bucket"),
            "{error}"
        );

        // Removal takes the declaration out and leaves the service's own
        // registration, dependency and settings.
        run(&RemoveCommand, root, &["files", "object-store"]);
        assert_eq!(snapshot(root), before);
    }

    #[test]
    fn a_worker_on_an_existing_bucket_is_wired_and_recorded() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        run(
            &AddCommand,
            root,
            &[
                "jobs",
                "object-store",
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                "jobs/",
            ],
        );
        let added = snapshot(root);
        let manifest: ApplicationManifestData = toml::from_str(&added[MANIFEST]).unwrap();
        let resources = manifest
            .projects
            .iter()
            .find(|p| p.name == "jobs")
            .and_then(|p| p.resources.clone())
            .unwrap();
        assert_eq!(resources.object_store.as_deref(), Some("s3"));
        assert_eq!(
            resources.object_store_bucket.as_deref(),
            Some("demo-shared-files")
        );
        assert_eq!(
            resources.object_store_prefixes,
            Some(vec!["jobs/".to_string()])
        );
        // The worker's queue backend is still its cache.
        assert_eq!(resources.cache.as_deref(), Some("bullmq"));
        assert!(added["src/modules/jobs/registrations.ts"].contains("  ObjectStore: {\n"));
        assert!(added[MANIFEST].contains("snake_case_name = \"jobs\"\n"));
        run(&RemoveCommand, root, &["jobs", "object-store"]);
        assert_eq!(snapshot(root), before);
    }

    #[test]
    fn a_compose_file_that_cannot_be_edited_in_place_is_left_byte_for_byte() {
        let root = fixture();
        let root = root.path();
        let compose_path = root.join(COMPOSE);
        // The API's whole environment is an alias: there is no block of its
        // own to add a setting to.
        let aliased = fs::read_to_string(&compose_path).unwrap().replace(
            "    environment:\n      <<: *common-environment\n      PORT: \"8000\" # the port the load balancer expects\n      REDIS_URL: redis://redis:6379/0\n",
            "    environment: *common-environment\n",
        );
        assert!(aliased.contains("    environment: *common-environment\n"));
        fs::write(&compose_path, &aliased).unwrap();
        let before = snapshot(root);
        run(&AddCommand, root, &["api", "object-store"]);
        let added = snapshot(root);
        assert_eq!(added[COMPOSE], aliased);
        // Everything else was still done.
        let paths = changed(&before, &added);
        assert!(paths.contains(&MANIFEST.to_string()), "{paths:?}");
        assert!(
            paths.contains(&"src/modules/api/registrations.ts".to_string()),
            "{paths:?}"
        );
        assert!(!paths.contains(&COMPOSE.to_string()));
        run(&RemoveCommand, root, &["api", "object-store"]);
        assert_eq!(snapshot(root), before);
    }

    #[test]
    fn local_settings_the_developer_set_survive_a_remove() {
        let root = fixture();
        let root = root.path();
        let env_local = root.join("src/modules/jobs/.env.local");
        fs::write(&env_local, "S3_BUCKET=shared-local\nQUEUE_NAME=jobs\n").unwrap();
        run(&AddCommand, root, &["jobs", "object-store"]);
        let added = fs::read_to_string(&env_local).unwrap();
        assert!(
            added.starts_with("S3_BUCKET=shared-local\nQUEUE_NAME=jobs\n"),
            "{added}"
        );
        assert!(added.contains("S3_URL=http://localhost:9000\n"), "{added}");
        assert!(!added.contains("S3_BUCKET=demo-jobs-dev"), "{added}");
        run(&RemoveCommand, root, &["jobs", "object-store"]);
        assert_eq!(
            fs::read_to_string(&env_local).unwrap(),
            "S3_BUCKET=shared-local\nQUEUE_NAME=jobs\n"
        );
    }

    #[test]
    fn a_workers_compose_processes_never_include_another_project() {
        use crate::core::docker::DockerCompose;

        let manifest: ApplicationManifestData = toml::from_str(
            &fs::read_to_string(fixture().path().join(MANIFEST))
                .unwrap()
                .replace("name = \"files\"", "name = \"jobs-service\""),
        )
        .unwrap();
        let compose = |services: &[&str]| -> DockerCompose {
            let body: String = services
                .iter()
                .map(|name| format!("  {name}:\n    image: demo\n"))
                .collect();
            serde_yml::from_str(&format!("services:\n{body}")).unwrap()
        };
        let keys = |services: &[&str]| {
            super::typescript_compose_keys(
                &compose(services),
                &manifest.projects,
                "jobs",
                &crate::core::manifest::ProjectType::Worker,
            )
        };
        // A generated stack.
        assert_eq!(
            keys(&["jobs-server", "jobs-worker", "api"]),
            ["jobs-server", "jobs-worker"]
        );
        // Hand-written stacks name the server after the project.
        assert_eq!(keys(&["jobs", "jobs-worker"]), ["jobs", "jobs-worker"]);
        // `jobs-service` is another project here, so it is not the worker's server.
        assert_eq!(keys(&["jobs-service", "jobs-worker"]), ["jobs-worker"]);
        assert!(keys(&["api"]).is_empty());
    }

    #[test]
    fn dryrun_writes_nothing() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        run(&AddCommand, root, &["jobs", "object-store", "--dryrun"]);
        run(
            &AddCommand,
            root,
            &[
                "files",
                "object-store",
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                "a/",
                "-n",
            ],
        );
        assert_eq!(snapshot(root), before);
    }

    #[test]
    fn what_cannot_be_added_is_refused_with_the_reason() {
        let root = fixture();
        let root = root.path();
        let before = snapshot(root);
        let refused = |args: &[&str]| try_run(&AddCommand, root, args).unwrap_err().to_string();

        // A TypeScript worker's cache is its queue backend.
        let error = refused(&["jobs", "cache"]);
        assert!(
            error.contains("TypeScript worker") && error.contains("change worker"),
            "{error}"
        );

        let error = refused(&[
            "api",
            "object-store",
            "--existing-bucket",
            "demo-shared-files",
        ]);
        assert!(error.contains("at least one --prefix"), "{error}");
        let error = refused(&["api", "object-store", "--prefix", "uploads/"]);
        assert!(error.contains("--existing-bucket"), "{error}");
        for bucket in [
            "arn:aws:s3:::demo",
            "s3://demo",
            "Demo_Bucket",
            "ab",
            "demo/files",
            "192.168.5.4",
        ] {
            let error = refused(&[
                "api",
                "object-store",
                "--existing-bucket",
                bucket,
                "--prefix",
                "a/",
            ]);
            assert!(error.contains("is not a bucket name"), "{bucket}: {error}");
        }
        for prefix in [
            "",
            "/uploads/",
            "*",
            "*/logs/",
            "tenants/${aws:userid}/",
            "my files/",
        ] {
            let error = refused(&[
                "api",
                "object-store",
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                prefix,
            ]);
            assert!(error.contains("cannot be used"), "{prefix:?}: {error}");
        }
        // The flags belong to object stores.
        for other in ["cache", "email"] {
            let error = refused(&[
                "api",
                other,
                "--existing-bucket",
                "demo-shared-files",
                "--prefix",
                "a/",
            ]);
            assert!(error.contains("do not apply"), "{other}: {error}");
        }
        assert_eq!(snapshot(root), before);
    }
}
