use anyhow::Result;
use clap::{ArgMatches, Command};
use create::CreateCommand;
use destroy::DestroyCommand;
use info::InfoCommand;

use crate::{CliCommand, core::command::command};

mod create;
mod destroy;
mod info;
pub(crate) mod utils;

#[derive(Debug)]
pub(crate) struct DeployCommand {
    create: CreateCommand,
    destroy: DestroyCommand,
    info: InfoCommand,
}

impl DeployCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            destroy: DestroyCommand::new(),
            info: InfoCommand::new(),
        }
    }
}

impl CliCommand for DeployCommand {
    fn command(&self) -> Command {
        command("deploy", "Deployment management")
            .subcommand(self.create.command())
            .subcommand(self.destroy.command())
            .subcommand(self.info.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("destroy", sub_matches)) => self.destroy.handler(sub_matches),
            Some(("info", sub_matches)) => self.info.handler(sub_matches),
            // Default to create for convenience - preserving existing behavior but usually nice to be explicit
            None => self.create.handler(matches),
            _ => unreachable!(),
        }
    }
}
