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
        rollout::print_rollout,
        types::FleetRollout,
    },
};

#[derive(Debug)]
pub(super) struct AdvanceCommand;

impl AdvanceCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for AdvanceCommand {
    fn command(&self) -> Command {
        command(
            "advance",
            "Resume a halted rollout: re-launch pending items and re-evaluate",
        )
        .long_about(
            "Resume a halted rollout: re-launch pending items and re-evaluate.\n\n\
                 A rollout halts by itself when a wave's failures exceed its threshold. Once\n\
                 the cause is fixed (a bad variable, a broken build), advance re-launches the\n\
                 items still pending and lets the rollout continue on its own. A running\n\
                 rollout needs no advancing; a healthy wave proceeds automatically.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the rollout"),
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
        let path = format!("/rollouts/{}/advance", urlencoding::encode(id));
        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&json!({})));
        }
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let rollout: FleetRollout = post_json(
            &auth_mode,
            &path,
            json!({}),
            Missing::Resource(format!("rollout '{}'", id)),
        )?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&rollout)?);
            return Ok(());
        }
        log_ok!(stdout, "Rollout resumed");
        print_rollout(&mut stdout, &rollout)?;
        writeln!(stdout)?;
        Ok(())
    }
}
