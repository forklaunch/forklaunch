use anyhow::{Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{
    CliCommand,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

use super::{
    mutation::{MutationRequest, nothing_to_change, run_mutation},
    resource_resolver::{fetch_resource_detail, resolve},
    types::ResourceConfig,
};

#[derive(Debug)]
pub(super) struct ConfigSetCommand;

impl ConfigSetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ConfigSetCommand {
    fn command(&self) -> Command {
        command(
            "config-set",
            "Change the configuration of a provisioned database, cache, or queue resource",
        )
        .arg(
            Arg::new("resource")
                .help("Resource identifier: <project-name>:<resource-type> (e.g. billing-service:database)")
                .required(true),
        )
        .arg(Arg::new("base_path").short('p').long("path").help("The application path"))
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment to target (for example: dev, staging, production)"),
        )
        .arg(
            Arg::new("resource_id")
                .long("resource-id")
                .help("Skip name resolution and address a resource by its platform id directly"),
        )
        .arg(Arg::new("engine").long("engine").help("Database/cache engine"))
        .arg(
            Arg::new("multi_az")
                .long("multi-az")
                .help("Enable Multi-AZ (database)")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("queue_type").long("queue-type").help("Queue type"))
        .arg(
            Arg::new("visibility_timeout")
                .long("visibility-timeout")
                .value_parser(clap::value_parser!(u32))
                .help("Queue visibility timeout (seconds)"),
        )
        .arg(
            Arg::new("message_retention_seconds")
                .long("message-retention-seconds")
                .value_parser(clap::value_parser!(u32))
                .help("Queue message retention (seconds)"),
        )
        .arg(Arg::new("encryption").long("encryption").help("Encryption setting"))
        .arg(Arg::new("kafka_version").long("kafka-version").help("Kafka version (queue)"))
        .arg(
            Arg::new("port")
                .long("port")
                .value_parser(clap::value_parser!(u16))
                .help("Port"),
        )
        .arg(
            Arg::new("distribution_strategy")
                .long("distribution-strategy")
                .help("Resource distribution strategy (centralized or distributed)"),
        )
        .arg(
            Arg::new("primary_region")
                .long("primary-region")
                .help("Primary region for the resource"),
        )
        .arg(
            Arg::new("snapshot_before_change")
                .long("snapshot-before-change")
                .help("Take a snapshot before applying the change (database)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .help("Skip the confirmation prompt (for CI/scripted use)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry_run")
                .long("dry-run")
                .help("Print the change without applying it")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .expect("--environment is required")
            .to_string();
        let resource_arg = matches
            .get_one::<String>("resource")
            .expect("<resource> is required");
        let resource_id_override = matches.get_one::<String>("resource_id").map(String::as_str);

        let requested_config = ResourceConfig {
            engine: matches.get_one::<String>("engine").cloned(),
            multi_az: if matches.get_flag("multi_az") { Some(true) } else { None },
            queue_type: matches.get_one::<String>("queue_type").cloned(),
            visibility_timeout: matches.get_one::<u32>("visibility_timeout").copied(),
            message_retention_seconds: matches.get_one::<u32>("message_retention_seconds").copied(),
            encryption: matches.get_one::<String>("encryption").cloned(),
            kafka_version: matches.get_one::<String>("kafka_version").cloned(),
            port: matches.get_one::<u16>("port").copied(),
            ..Default::default()
        };
        let distribution_strategy = matches.get_one::<String>("distribution_strategy").cloned();
        let primary_region = matches.get_one::<String>("primary_region").cloned();

        // Fail before any network call — resolving the resource and fetching its
        // current detail are both wasted if there's nothing to change.
        if nothing_to_change(&requested_config, &distribution_strategy, &primary_region) {
            bail!("nothing to change — pass at least one config or distribution flag");
        }

        let resolved = resolve(
            &manifest,
            &application_id,
            &environment,
            resource_arg,
            resource_id_override,
        )?;

        let current = fetch_resource_detail(&resolved.id)?;

        run_mutation(MutationRequest {
            resource_id: resolved.id,
            current,
            requested_config,
            distribution_strategy,
            primary_region,
            snapshot_before_change: if matches.get_flag("snapshot_before_change") {
                Some(true)
            } else {
                None
            },
            skip_confirm: matches.get_flag("yes"),
            dry_run: matches.get_flag("dry_run"),
        })
    }
}
