use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, patch_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::{INSTANCE_SIZES, ManagedInstance, UPDATE_POLICIES},
    },
};

#[derive(Debug)]
pub(super) struct UpdateCommand;

impl UpdateCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for UpdateCommand {
    fn command(&self) -> Command {
        command(
            "update",
            "Change an instance's compute tier or whether fleet rollouts may update it",
        )
        .long_about(
            "Change an instance's compute tier or whether fleet rollouts may update it.\n\n\
             --size redeploys the CURRENT version at the new tier: no data, key or address\n\
             changes. The row records pendingUpdate=size until the platform reports the\n\
             deploy finished; follow it with `instance get` or `instance deployments`.\n\
             Allowed while the instance is awaiting_claim, active or suspended, and refused\n\
             while a fleet rollout is updating it.\n\n\
             --update-policy deferred makes every fleet rollout skip this instance until you\n\
             set it back to auto. --deferred-until records when the deferral lapses;\n\
             --clear-deferral removes it. Policy changes alone do not redeploy.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
        )
        .arg(
            Arg::new("size")
                .long("size")
                .value_parser(INSTANCE_SIZES.to_vec())
                .help("Compute tier for every service of the instance (redeploys)"),
        )
        .arg(
            Arg::new("update_policy")
                .long("update-policy")
                .value_parser(UPDATE_POLICIES.to_vec())
                .help("Whether fleet rollouts may update this instance"),
        )
        .arg(
            Arg::new("deferred_until")
                .long("deferred-until")
                .conflicts_with("clear_deferral")
                .help("ISO-8601 timestamp until which fleet rollouts skip this instance"),
        )
        .arg(
            Arg::new("clear_deferral")
                .long("clear-deferral")
                .help("Remove the deferral timestamp")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}", urlencoding::encode(id));

        let mut body = serde_json::Map::new();
        if let Some(size) = matches.get_one::<String>("size") {
            body.insert("instanceSize".into(), json!(size));
        }
        if let Some(policy) = matches.get_one::<String>("update_policy") {
            body.insert("updatePolicy".into(), json!(policy));
        }
        if let Some(until) = matches.get_one::<String>("deferred_until") {
            body.insert("updateDeferredUntil".into(), json!(until));
        }
        if matches.get_flag("clear_deferral") {
            body.insert("updateDeferredUntil".into(), Value::Null);
        }
        if body.is_empty() {
            bail!(
                "nothing to change — pass --size, --update-policy, --deferred-until or --clear-deferral"
            );
        }
        let body = Value::Object(body);

        if matches.get_flag("dryrun") {
            return print_dryrun("PATCH", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let instance: ManagedInstance =
            patch_json(&path, body, Missing::Resource(format!("instance '{}'", id)))?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&instance)?);
            return Ok(());
        }

        match instance.pending_update.as_deref() {
            Some("size") => {
                log_ok!(
                    stdout,
                    "Resizing to {}: redeploying the current version",
                    instance.instance_size.as_deref().unwrap_or("the new tier")
                );
                log_info!(
                    stdout,
                    "Follow it with `forklaunch managed instance get --id {}` (pendingUpdate clears when the deploy lands) or `instance deployments --id {}`.",
                    id,
                    id
                );
            }
            _ => {
                log_ok!(stdout, "Saved; no redeploy was needed");
            }
        }
        if let Some(policy) = instance.update_policy.as_deref() {
            log_info!(
                stdout,
                "Fleet updates: {}{}",
                policy,
                instance
                    .update_deferred_until
                    .as_deref()
                    .map(|until| format!(" (until {})", until))
                    .unwrap_or_default()
            );
        }
        writeln!(stdout)?;
        Ok(())
    }
}
