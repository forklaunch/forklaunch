use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod issues;
mod logs;
mod status;

use issues::IssuesCommand;
use logs::LogsCommand;
use status::StatusCommand;

#[derive(Debug)]
pub(crate) struct ObserveCommand {
    status: StatusCommand,
    logs: LogsCommand,
    issues: IssuesCommand,
}

impl ObserveCommand {
    pub(crate) fn new() -> Self {
        Self {
            status: StatusCommand::new(),
            logs: LogsCommand::new(),
            issues: IssuesCommand::new(),
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
        .subcommand(self.logs.command())
        .subcommand(self.issues.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("logs", sub_matches)) => self.logs.handler(sub_matches),
            Some(("issues", sub_matches)) => self.issues.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
