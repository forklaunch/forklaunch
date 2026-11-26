use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, post_text, print_dryrun, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct ResumeCommand;

impl ResumeCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ResumeCommand {
    fn command(&self) -> Command {
        command(
            "resume",
            "Re-queue a launch that failed or is holding for deployment approval",
        )
        .long_about(
            "Re-queue a launch that failed or is holding for deployment approval.\n\n\
             This is the LAUNCH retry: from provisioning_failed it re-enters provisioning and\n\
             re-runs the idempotent step list (nothing already created is created twice); for\n\
             a launch parked with launchApprovalState=pending it re-queues the deploy.\n\n\
             A reset that failed is retried with `instance reset`, not with this — the launch\n\
             path mints no claim link for an instance that is still claimed.",
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
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}/resume-provisioning", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&json!({})));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let message = post_text(
            &path,
            json!({}),
            Missing::Resource(format!(
                "instance '{}' holding for approval or in provisioning_failed",
                id
            )),
        )?;

        log_ok!(stdout, "{}", message);
        log_info!(
            stdout,
            "Follow it with `forklaunch managed instance get --id {}`.",
            id
        );
        writeln!(stdout)?;
        Ok(())
    }
}
