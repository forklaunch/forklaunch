use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::StateAccepted,
    },
};

#[derive(Debug)]
pub(super) struct ApplyVariablesCommand;

impl ApplyVariablesCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ApplyVariablesCommand {
    fn command(&self) -> Command {
        command(
            "apply-variables",
            "Redeploy the current version so newly written variables reach the tasks",
        )
        .long_about(
            "Redeploy the current version so newly written variables reach the tasks.\n\n\
             Writing a variable (`instance vars set`, `template vars set`, or the application's\n\
             environment editor) does not restart anything. This records pendingUpdate=variables\n\
             and redeploys the same version with the current values; the marker clears when the\n\
             platform reports the deploy finished, and stays (with lastError set) if it failed,\n\
             so a retry is the same command again.\n\n\
             Allowed while the instance is awaiting_claim, active or suspended; refused while a\n\
             fleet rollout is updating it.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
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
        let path = format!("/instances/{}/apply-variables", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&json!({})));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let accepted: StateAccepted = post_json(
            &auth_mode,
            &path,
            json!({}),
            Missing::Resource(format!("instance '{}'", id)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&accepted)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "Applying variables: redeploying the current version (state {})",
            accepted.state.as_deref().unwrap_or("unchanged")
        );
        log_info!(
            stdout,
            "Follow it with `forklaunch managed instance get --id {}`; pendingUpdate clears when the deploy lands.",
            id
        );
        writeln!(stdout)?;
        Ok(())
    }
}
