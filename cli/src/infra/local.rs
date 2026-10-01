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

fn args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("service")
            .required(true)
            .help("The service to change, by name"),
    )
    .arg(
        Arg::new("type")
            .required(true)
            .help("What to add: object-store (s3) or cache (redis)"),
    )
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
    if project.r#type != ProjectType::Service {
        bail!(
            "'{service}' is not a service; `infra add` changes services (use `forklaunch change worker` for workers)"
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
    if project.r#type != ProjectType::Service {
        bail!("'{service}' is not a service; capabilities are added to services");
    }
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

    let service_path = app_root.join(&manifest.modules_path).join(&service);
    let mut edit = CapabilityEdit::new(
        &app_root,
        &manifest.app_name,
        &service,
        &service_path,
        service_capabilities.clone(),
        app_capabilities,
    );
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
    edit.commit()?;
    println!(
        "{} {} {service}: {}",
        if adding { "Added" } else { "Removed" },
        capability.id,
        capability.summary
    );
    Ok(())
}

#[derive(Debug)]
pub(super) struct AddCommand;

impl CliCommand for AddCommand {
    fn command(&self) -> Command {
        args(command(
            "add",
            "Add a capability to a service: writes its setup code, local stand-in and manifest resource",
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
    use super::parse_type;
    use crate::constants::Infrastructure;

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
