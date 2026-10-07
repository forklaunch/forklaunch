use std::{io::Write, thread::sleep, time::Duration};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{command::command, hmac::AuthMode},
    managed::{
        client::{
            Missing, extract_list, get_value_if_supported, post_json, print_dryrun,
            require_managed_mode, resolve_managed_auth,
        },
        types::ManagedInstance,
    },
};

/// How long to wait between instance-list polls while following a provision. Kept in
/// the 3 to 5 second range the sibling `deploy create` status loop uses, so the two
/// commands feel the same to an operator watching either one.
const POLL_INTERVAL_SECS: u64 = 4;

#[derive(Debug)]
pub(super) struct CreateCommand;

impl CreateCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command(
            "create",
            "Launch a managed instance of a published template",
        )
        .long_about(
            "Launch a managed instance of a published template.\n\n\
                 This provisions a deployment for one end customer. The template must already\n\
                 have a published version — launching from a draft or unbuilt template fails.\n\n\
                 Provisioning is asynchronous: the instance comes back in a provisioning state\n\
                 and becomes claimable once it is up. Once it reaches awaiting_claim, reveal\n\
                 its one-time claim link with `forklaunch managed instance claim-link`.",
        )
        .arg(
            Arg::new("template")
                .long("template")
                .required(true)
                .help("Slug of the published template to launch"),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .required(true)
                .help("Region to provision the instance in (for example: us-west-2)"),
        )
        .arg(
            Arg::new("instance-size")
                .long("instance-size")
                .value_parser([
                    "pico", "nano", "micro", "small", "medium", "large", "xlarge", "2xlarge",
                ])
                .help(
                    "Compute tier for the instance's services. Omitted => the managed default (pico, ~0.1 vCPU / 256 MB). Use a larger tier to give the instance more compute.",
                ),
        )
        .arg(
            Arg::new("no-wait")
                .long("no-wait")
                .help(
                    "Return as soon as the instance is launched instead of waiting for provisioning to finish",
                )
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

        let template = matches
            .get_one::<String>("template")
            .context("--template is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let instance_size = matches.get_one::<String>("instance-size");

        let mut body = json!({ "templateSlug": template, "region": region });
        if let Some(size) = instance_size {
            body["instanceSize"] = json!(size);
        }

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", "/instances", Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        // A 404 here is the platform saying there is no *published* template by that
        // slug, which is a different problem from the endpoint being absent — name the
        // template so the message points at the real cause.
        let instance: ManagedInstance = post_json(
            &auth_mode,
            "/instances",
            body,
            Missing::Resource(format!("published template '{}'", template)),
        )?;

        if matches.get_flag("json") {
            // --json is a one-shot machine-readable dump of the launched instance, so it
            // returns immediately rather than following provisioning.
            println!("{}", serde_json::to_string_pretty(&instance)?);
            return Ok(());
        }

        let wait = !matches.get_flag("no-wait");

        log_ok!(
            stdout,
            "Launched instance of '{}' in {}",
            instance
                .template_slug
                .as_deref()
                .unwrap_or(template.as_str()),
            instance.region.as_deref().unwrap_or(region.as_str())
        );
        if let Some(id) = instance.id.as_deref() {
            log_info!(stdout, "Instance id: {}", id);
        }
        if let Some(host) = instance.host.as_deref() {
            log_info!(stdout, "Host: {}", host);
        }

        // Without an id there is nothing to poll for (the control plane returns it on
        // create), so drop back to the background-provisioning guidance either way.
        let id = instance.id.as_deref();
        if !wait || id.is_none() {
            if let Some(state) = instance.state.as_deref() {
                log_info!(stdout, "State: {}", state);
            }
            writeln!(stdout)?;
            log_info!(
                stdout,
                "Provisioning runs in the background. Watch it with `forklaunch managed instance list`."
            );
            if let Some(id) = id {
                log_info!(
                    stdout,
                    "Once it reaches awaiting_claim, hand the customer their claim link: forklaunch managed instance claim-link --id {}",
                    id
                );
            }
            return Ok(());
        }

        writeln!(stdout)?;
        log_info!(stdout, "Waiting for provisioning to finish...");
        wait_for_provisioning(
            &auth_mode,
            id.expect("id presence checked above"),
            instance.state.as_deref(),
            &mut stdout,
        )
    }
}

/// A terminal provisioning result.
enum Outcome {
    /// `awaiting_claim`: the instance is up and ready to hand to the customer.
    AwaitingClaim,
    /// `provisioning_failed`: provisioning gave up.
    Failed,
}

/// Maps a state to its terminal outcome, or `None` while it is still in progress.
///
/// Only the two states the task defines as terminal end the wait; every other state
/// (including `provisioning` itself) is treated as still in progress so a state the
/// platform adds later does not end the wait prematurely.
fn terminal_outcome(state: &str) -> Option<Outcome> {
    match state {
        "awaiting_claim" => Some(Outcome::AwaitingClaim),
        "provisioning_failed" => Some(Outcome::Failed),
        _ => None,
    }
}

/// Fetches the current instance list, preferring the dedicated list endpoint and
/// falling back to the managed-mode summary, exactly as `managed instance list` does.
/// There is no single-instance GET endpoint on the control plane today, so following
/// one instance means polling the list and filtering by id.
fn fetch_instances(auth_mode: &AuthMode) -> Result<Vec<ManagedInstance>> {
    match get_value_if_supported(auth_mode, "/instances")? {
        Some(value) => extract_list::<ManagedInstance>(value, &["instances"]),
        None => Ok(require_managed_mode(auth_mode)?.instances),
    }
}

/// Polls the instance list until this instance reaches a terminal state, printing each
/// CHANGED state as it goes. Reuses the shape of the sibling `deploy create` status
/// loop (fixed poll interval, last-value dedupe, terminal detection).
fn wait_for_provisioning(
    auth_mode: &AuthMode,
    id: &str,
    initial_state: Option<&str>,
    stdout: &mut StandardStream,
) -> Result<()> {
    let mut last_state: Option<String> = None;

    // The create response already carries a state (typically `provisioning`); print it
    // now so the operator sees progress before the first poll interval elapses, and in
    // case the instance is already terminal.
    if let Some(state) = initial_state {
        log_info!(stdout, "State: {}", state);
        last_state = Some(state.to_string());
        if let Some(outcome) = terminal_outcome(state) {
            return finish(outcome, id, None, stdout);
        }
    }

    loop {
        sleep(Duration::from_secs(POLL_INTERVAL_SECS));

        let instances = fetch_instances(auth_mode)?;
        let instance = instances
            .into_iter()
            .find(|instance| instance.id.as_deref() == Some(id));

        // Right after create the instance can be briefly absent from the list, and a
        // partial control plane may omit the state; in both cases keep polling rather
        // than failing.
        let Some(instance) = instance else {
            continue;
        };
        let Some(state) = instance.state.clone() else {
            continue;
        };

        if last_state.as_deref() != Some(state.as_str()) {
            log_info!(stdout, "State: {}", state);
            last_state = Some(state.clone());
        }

        if let Some(outcome) = terminal_outcome(&state) {
            return finish(outcome, id, instance.last_error.as_deref(), stdout);
        }
    }
}

/// Prints the closing message for a terminal outcome and, on failure, exits non-zero.
fn finish(
    outcome: Outcome,
    id: &str,
    last_error: Option<&str>,
    stdout: &mut StandardStream,
) -> Result<()> {
    match outcome {
        Outcome::AwaitingClaim => {
            writeln!(stdout)?;
            log_ok!(stdout, "Instance provisioned and awaiting claim.");
            log_info!(
                stdout,
                "Hand the customer their one-time claim link: forklaunch managed instance claim-link --id {}",
                id
            );
            Ok(())
        }
        Outcome::Failed => {
            writeln!(stdout)?;
            log_error!(stdout, "Provisioning failed.");
            // `lastError` is a bonus field on the list projection; print it when the
            // control plane includes it, otherwise point the operator at the list.
            if let Some(error) = last_error {
                log_error!(stdout, "Error: {}", error);
            } else {
                log_info!(
                    stdout,
                    "See `forklaunch managed instance list` for details."
                );
            }
            bail!("instance provisioning failed");
        }
    }
}
