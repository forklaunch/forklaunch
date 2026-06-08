use anyhow::Result;
use clap::{ArgMatches, Command};
use create::CreateCommand;
use info::InfoCommand;

use crate::{CliCommand, core::command::command};

mod create;
mod info;
mod git;
mod manifest_generator;
pub(crate) mod s3_upload;

#[derive(Debug)]
pub(crate) struct ReleaseCommand {
    create: CreateCommand,
    info: InfoCommand,
}

impl ReleaseCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            info: InfoCommand::new(),
        }
    }
}

impl CliCommand for ReleaseCommand {
    fn command(&self) -> Command {
        command("release", "Release management")
            .subcommand(self.create.command())
            .subcommand(self.info.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("info", sub_matches)) => self.info.handler(sub_matches),
            // Default to create for convenience
            None => self.create.handler(matches),
            _ => unreachable!(),
        }
    }
}
