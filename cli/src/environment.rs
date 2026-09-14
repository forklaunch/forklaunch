use anyhow::Result;
use clap::{ArgMatches, Command};
use status::StatusCommand;
use sync::SyncCommand;
use validate::ValidateCommand;

use crate::{CliCommand, core::command::command};

pub(crate) mod status;
pub(crate) mod sync;
pub(crate) mod validate;

#[derive(Debug)]
pub(crate) struct EnvironmentCommand {
    validate: ValidateCommand,
    sync: SyncCommand,
    status: StatusCommand,
}

impl EnvironmentCommand {
    pub(crate) fn new() -> Self {
        Self {
            validate: ValidateCommand::new(),
            sync: SyncCommand::new(),
            status: StatusCommand::new(),
        }
    }
}

impl CliCommand for EnvironmentCommand {
    fn command(&self) -> Command {
        command(
            "environment",
            "Manage environment variables across workspace projects",
        )
        .alias("env")
        .subcommand_required(true)
        .subcommand(self.validate.command())
        .subcommand(self.sync.command())
        .subcommand(self.status.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("validate", sub_matches)) => self.validate.handler(sub_matches),
            Some(("sync", sub_matches)) => self.sync.handler(sub_matches),
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
