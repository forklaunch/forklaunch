use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod status;

use status::StatusCommand;

#[derive(Debug)]
pub(crate) struct ObserveCommand {
    status: StatusCommand,
}

impl ObserveCommand {
    pub(crate) fn new() -> Self {
        Self {
            status: StatusCommand::new(),
        }
    }
}

impl CliCommand for ObserveCommand {
    fn command(&self) -> Command {
        command(
            "observe",
            "Inspect logs, metrics, traces, and live health for a ForkLaunch application",
        )
        .subcommand(self.status.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
