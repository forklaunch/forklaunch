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
pub(super) struct ResizeCommand;

impl ResizeCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ResizeCommand {
    fn command(&self) -> Command {
        command(
            "resize",
            "Resize a provisioned database, cache, or queue resource",
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
        .arg(
            Arg::new("instance_class")
                .long("instance-class")
                .help("Database instance class (e.g. db.t3.small)"),
        )
        .arg(
            Arg::new("allocated_storage")
                .long("allocated-storage")
                .value_parser(clap::value_parser!(u32))
                .help("Allocated storage in GB (database)"),
        )
        .arg(Arg::new("node_type").long("node-type").help("Cache node type"))
        .arg(
            Arg::new("num_cache_nodes")
                .long("num-cache-nodes")
                .value_parser(clap::value_parser!(u32))
                .help("Number of cache nodes"),
        )
        .arg(
            Arg::new("number_of_broker_nodes")
                .long("number-of-broker-nodes")
                .value_parser(clap::value_parser!(u32))
                .help("Number of broker nodes (queue)"),
        )
        .arg(
            Arg::new("ebs_storage_size")
                .long("ebs-storage-size")
                .value_parser(clap::value_parser!(u32))
                .help("EBS storage size in GB (queue)"),
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
            instance_class: matches.get_one::<String>("instance_class").cloned(),
            allocated_storage: matches.get_one::<u32>("allocated_storage").copied(),
            node_type: matches.get_one::<String>("node_type").cloned(),
            num_cache_nodes: matches.get_one::<u32>("num_cache_nodes").copied(),
            number_of_broker_nodes: matches.get_one::<u32>("number_of_broker_nodes").copied(),
            ebs_storage_size: matches.get_one::<u32>("ebs_storage_size").copied(),
            ..Default::default()
        };

        // Fail before any network call — resolving the resource and fetching its
        // current detail are both wasted if there's nothing to change.
        if nothing_to_change(&requested_config, &None, &None) {
            bail!("nothing to change — pass at least one sizing flag");
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
            distribution_strategy: None,
            primary_region: None,
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
