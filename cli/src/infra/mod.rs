use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

pub(crate) mod capabilities;
mod config_set;
mod delete;
mod in_place;
mod lifecycle;
mod list;
mod local;
mod mutation;
mod resize;
mod resource_resolver;
mod status;
mod stop;
mod types;

use config_set::ConfigSetCommand;
use delete::DeleteCommand;
use list::ListCommand;
use local::{AddCommand, RemoveCommand};
use resize::ResizeCommand;
use status::StatusCommand;
use stop::StopCommand;

#[derive(Debug)]
pub(crate) struct InfraCommand {
    add: AddCommand,
    remove: RemoveCommand,
    list: ListCommand,
    status: StatusCommand,
    resize: ResizeCommand,
    config_set: ConfigSetCommand,
    stop: StopCommand,
    delete: DeleteCommand,
}

impl InfraCommand {
    pub(crate) fn new() -> Self {
        Self {
            add: AddCommand,
            remove: RemoveCommand,
            list: ListCommand::new(),
            status: StatusCommand::new(),
            resize: ResizeCommand::new(),
            config_set: ConfigSetCommand::new(),
            stop: StopCommand::new(),
            delete: DeleteCommand::new(),
        }
    }
}

impl CliCommand for InfraCommand {
    fn command(&self) -> Command {
        command(
            "infra",
            "Inspect and manage provisioned database, cache, and queue infrastructure",
        )
        .subcommand(self.add.command())
        .subcommand(self.remove.command())
        .subcommand(self.list.command())
        .subcommand(self.status.command())
        .subcommand(self.resize.command())
        .subcommand(self.config_set.command())
        .subcommand(self.stop.command())
        .subcommand(self.delete.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("add", sub_matches)) => self.add.handler(sub_matches),
            Some(("remove", sub_matches)) => self.remove.handler(sub_matches),
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("resize", sub_matches)) => self.resize.handler(sub_matches),
            Some(("config-set", sub_matches)) => self.config_set.handler(sub_matches),
            Some(("stop", sub_matches)) => self.stop.handler(sub_matches),
            Some(("delete", sub_matches)) => self.delete.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
