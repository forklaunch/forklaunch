use anyhow::Result;
use clap::{ArgMatches, Command};
use pull::PullCommand;
use push::PushCommand;
use set::SetCommand;

use crate::{CliCommand, core::command::command};

mod pull;
mod push;
mod set;

#[derive(Debug)]
pub(crate) struct ConfigCommand {
    pull: PullCommand,
    push: PushCommand,
    set: SetCommand,
}

impl ConfigCommand {
    pub(crate) fn new() -> Self {
        Self {
            pull: PullCommand::new(),
            push: PushCommand::new(),
            set: SetCommand::new(),
        }
    }
}

impl CliCommand for ConfigCommand {
    fn command(&self) -> Command {
        command(
            "config",
            "Pull and push environment configuration for an application",
        )
        .subcommand_required(true)
        .subcommand(self.pull.command())
        .subcommand(self.push.command())
        .subcommand(self.set.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("pull", matches)) => self.pull.handler(matches),
            Some(("push", matches)) => self.push.handler(matches),
            Some(("set", matches)) => self.set.handler(matches),
            _ => unreachable!(),
        }
    }
}
