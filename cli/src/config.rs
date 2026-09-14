use anyhow::Result;
use clap::{ArgMatches, Command};
use prune::PruneCommand;
use pull::PullCommand;
use push::PushCommand;
use set::SetCommand;
use unset::UnsetCommand;

use crate::{CliCommand, core::command::command};

mod declared;
mod prune;
mod pull;
mod push;
mod set;
mod unset;

#[derive(Debug)]
pub(crate) struct ConfigCommand {
    prune: PruneCommand,
    pull: PullCommand,
    push: PushCommand,
    set: SetCommand,
    unset: UnsetCommand,
}

impl ConfigCommand {
    pub(crate) fn new() -> Self {
        Self {
            prune: PruneCommand::new(),
            pull: PullCommand::new(),
            push: PushCommand::new(),
            set: SetCommand::new(),
            unset: UnsetCommand::new(),
        }
    }
}

impl CliCommand for ConfigCommand {
    fn command(&self) -> Command {
        command(
            "config",
            "Read and write environment configuration for an application",
        )
        .subcommand_required(true)
        .subcommand(self.prune.command())
        .subcommand(self.pull.command())
        .subcommand(self.push.command())
        .subcommand(self.set.command())
        .subcommand(self.unset.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("prune", matches)) => self.prune.handler(matches),
            Some(("pull", matches)) => self.pull.handler(matches),
            Some(("push", matches)) => self.push.handler(matches),
            Some(("set", matches)) => self.set.handler(matches),
            Some(("unset", matches)) => self.unset.handler(matches),
            _ => unreachable!(),
        }
    }
}
