use anyhow::Result;
use clap::{ArgMatches, Command};
use library::LibraryCommand;
use router::RouterCommand;
use service::ServiceCommand;
use worker::WorkerCommand;

use crate::{CliCommand, core::command::command};

pub(crate) mod library;
pub(crate) mod router;
pub(crate) mod service;
pub(crate) mod worker;

// TODO: add injected token into struct
#[derive(Debug)]
pub(crate) struct DeleteCommand {
    library: LibraryCommand,
    service: ServiceCommand,
    router: RouterCommand,
    worker: WorkerCommand,
}

impl DeleteCommand {
    pub(crate) fn new() -> Self {
        Self {
            library: LibraryCommand::new(),
            service: ServiceCommand::new(),
            router: RouterCommand::new(),
            worker: WorkerCommand::new(),
        }
    }
}

impl CliCommand for DeleteCommand {
    fn command(&self) -> Command {
        command("delete", "Delete a forklaunch project")
            .alias("del")
            .subcommand_required(true)
            .subcommand(self.library.command())
            .subcommand(self.service.command())
            .subcommand(self.router.command())
            .subcommand(self.worker.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("library", sub_matches)) => self.library.handler(sub_matches),
            Some(("service", sub_matches)) => self.service.handler(sub_matches),
            Some(("router", sub_matches)) => self.router.handler(sub_matches),
            Some(("worker", sub_matches)) => self.worker.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_subcommands_describe_deletion() {
        let command = DeleteCommand::new().command();
        for subcommand in command.get_subcommands() {
            let about = subcommand.get_about().map(ToString::to_string).unwrap_or_default();
            assert!(about.starts_with("Delete "), "{}: {about}", subcommand.get_name());
            for argument in subcommand.get_arguments() {
                let help = argument.get_help().map(ToString::to_string).unwrap_or_default();
                assert!(!help.contains("initialize") && !help.contains("eject"), "{} --{}: {help}", subcommand.get_name(), argument.get_id());
            }
        }
    }
}
