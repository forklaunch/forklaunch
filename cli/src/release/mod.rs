use anyhow::Result;
use clap::{ArgMatches, Command};
use create::CreateCommand;
use eject::EjectCommand;
use info::InfoCommand;
use list::ListCommand;

use crate::{CliCommand, core::command::command};

mod create;
mod eject;
mod info;
mod list;
mod git;
mod manifest_generator;
pub(crate) mod s3_upload;
mod shared;

#[derive(Debug)]
pub(crate) struct ReleaseCommand {
    create: CreateCommand,
    info: InfoCommand,
    list: ListCommand,
    eject: EjectCommand,
}

impl ReleaseCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            info: InfoCommand::new(),
            list: ListCommand::new(),
            eject: EjectCommand::new(),
        }
    }
}

impl CliCommand for ReleaseCommand {
    fn command(&self) -> Command {
        command("release", "Release management")
            .subcommand(self.create.command())
            .subcommand(self.info.command())
            .subcommand(self.list.command())
            .subcommand(self.eject.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("info", sub_matches)) => self.info.handler(sub_matches),
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("eject", sub_matches)) => self.eject.handler(sub_matches),
            // Default to create for convenience
            None => self.create.handler(matches),
            _ => unreachable!(),
        }
    }
}
