use anyhow::Result;
use clap::{ArgMatches, Command};
use create::CreateCommand;

use crate::{CliCommand, core::command::command};

mod create;

#[derive(Debug)]
pub(crate) struct AppCommand {
    create: CreateCommand,
}

impl AppCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
        }
    }
}

impl CliCommand for AppCommand {
    fn command(&self) -> Command {
        command("app", "Manage platform applications")
            .subcommand_required(true)
            .subcommand(self.create.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", matches)) => self.create.handler(matches),
            _ => unreachable!(),
        }
    }
}
