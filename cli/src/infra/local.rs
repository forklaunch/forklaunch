//! `forklaunch infra add|remove <service> <type>`: add or remove a capability
//! in a service's code.
//!
//! This is the documented way in for agents and people alike: it writes the
//! setup code (registrations.ts), the local stand-in (docker-compose), the
//! env files, the package dependency, the manifest resource and the test
//! utilities — the same edits `forklaunch change service --infrastructure`
//! makes, which it delegates to. The difference is the verb: `change service`
//! takes the full set of infrastructure a service should have, `infra add`
//! and `infra remove` take one change.

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{
    CliCommand,
    change::service::ServiceCommand,
    constants::Infrastructure,
    core::{command::command, manifest::ProjectType, validate::require_manifest},
};

/// What `infra add` can add: local infrastructure the service runs itself,
/// or a platform-held capability reached through the instance gateway.
enum Addable {
    Infrastructure(Infrastructure),
    Capability(&'static super::capabilities::Capability),
}

fn parse_addable(name: &str) -> Result<Addable> {
    if let Some(capability) = super::capabilities::find(name) {
        return Ok(Addable::Capability(capability));
    }
    parse_type(name).map(Addable::Infrastructure).map_err(|_| {
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
        anyhow::anyhow!(
            "unknown type '{name}'; supported: object-store (s3), cache (redis)"
        )
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

/// The service's infrastructure after this change, as `change service` takes it.
fn desired_infrastructure(
    matches: &ArgMatches,
    adding: bool,
) -> Result<(String, Vec<String>)> {
    let service = matches.get_one::<String>("service").unwrap().clone();
    let wanted = parse_type(matches.get_one::<String>("type").unwrap())?;
    let (_, manifest) = require_manifest(matches)?;
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
    let resources = project.resources.as_ref();
    let mut active: Vec<String> = [
        resources.and_then(|r| r.cache.clone()),
        resources.and_then(|r| r.queue.clone()),
        resources.and_then(|r| r.object_store.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let id = wanted.to_string();
    let present = active.contains(&id);
    match (adding, present) {
        (true, true) => bail!("'{service}' already has {id}"),
        (false, false) => bail!("'{service}' does not have {id}"),
        (true, false) => active.push(id),
        (false, true) => active.retain(|a| a != &id),
    }
    Ok((service, active))
}

fn delegate(matches: &ArgMatches, service: &str, infrastructure: &[String]) -> Result<()> {
    let mut argv = vec!["service".to_string(), "-p".to_string(), service.to_string()];
    if let Some(root) = matches.get_one::<String>("base_path") {
        // `change service` resolves -p against the current directory.
        let root = std::path::Path::new(root);
        argv[2] = root.join(service).to_string_lossy().to_string();
    }
    if infrastructure.is_empty() {
        // An empty set is spelled as the flag with no values.
        argv.push("--infrastructure".to_string());
    }
    for item in infrastructure {
        argv.push("--infrastructure".to_string());
        argv.push(item.clone());
    }
    argv.push("--confirm".to_string());
    if matches.get_flag("dryrun") {
        argv.push("--dryrun".to_string());
    }
    let command = ServiceCommand::new();
    let service_matches = command
        .command()
        .version(env!("CARGO_PKG_VERSION"))
        .try_get_matches_from(argv)?;
    command.handler(&service_matches)
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
    if adding {
        (capability.add)(&mut edit)?;
        edit.ensure_gateway_wiring(capability.id)?;
    } else {
        (capability.remove)(&mut edit)?;
        if capability.receives_events {
            edit.remove_platform_events(capability.id)?;
        }
        edit.release_gateway_wiring(capability.id)?;
    }

    // The manifest, edited as TOML so everything else in it is kept.
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    let projects = value
        .get_mut("projects")
        .and_then(|p| p.as_array_mut())
        .context("manifest has no projects")?;
    for p in projects.iter_mut() {
        if p.get("name").and_then(|n| n.as_str()) != Some(service.as_str()) {
            continue;
        }
        let table = p.as_table_mut().context("project is not a table")?;
        let resources = table
            .entry("resources")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .context("resources is not a table")?;
        if service_capabilities.is_empty() {
            resources.remove("capabilities");
        } else {
            resources.insert(
                "capabilities".to_string(),
                toml::Value::Array(
                    service_capabilities
                        .iter()
                        .map(|c| toml::Value::String(c.clone()))
                        .collect(),
                ),
            );
        }
    }
    edit.write(manifest_path, toml::to_string_pretty(&value)?);

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
        let (service, infrastructure) = desired_infrastructure(matches, true)?;
        delegate(matches, &service, &infrastructure)
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
        let (service, infrastructure) = desired_infrastructure(matches, false)?;
        delegate(matches, &service, &infrastructure)
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
        assert!(parse_type("email").unwrap_err().to_string().contains("supported"));
    }
}
