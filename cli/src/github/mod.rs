use anyhow::Result;
use clap::{ArgMatches, Command};
use connect::ConnectCommand;
use disconnect::DisconnectCommand;
use install::InstallCommand;
use status::StatusCommand;

use crate::{CliCommand, core::command::command};

mod connect;
mod disconnect;
mod install;
mod status;

#[derive(Debug)]
pub(crate) struct GithubCommand {
    connect: ConnectCommand,
    disconnect: DisconnectCommand,
    install: InstallCommand,
    status: StatusCommand,
}

impl GithubCommand {
    pub(crate) fn new() -> Self {
        Self {
            connect: ConnectCommand::new(),
            disconnect: DisconnectCommand::new(),
            install: InstallCommand::new(),
            status: StatusCommand::new(),
        }
    }
}

impl CliCommand for GithubCommand {
    fn command(&self) -> Command {
        command(
            "github",
            "Connect GitHub repositories and configure autodeploy",
        )
        .subcommand(self.install.command())
        .subcommand(self.status.command())
        .subcommand(self.connect.command())
        .subcommand(self.disconnect.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("install", sub_matches)) => self.install.handler(sub_matches),
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("connect", sub_matches)) => self.connect.handler(sub_matches),
            Some(("disconnect", sub_matches)) => self.disconnect.handler(sub_matches),
            _ => {
                GithubCommand::new().command().print_help()?;
                Ok(())
            }
        }
    }
}
