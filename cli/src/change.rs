use anyhow::Result;
use application::ApplicationCommand;
use clap::{ArgMatches, Command};
use cli_version::CliVersionCommand;
use library::LibraryCommand;
use router::RouterCommand;
use service::ServiceCommand;
use worker::WorkerCommand;

use crate::{CliCommand, core::command::command};

pub(crate) mod application;
pub(crate) mod cli_version;
pub(crate) mod core;
pub(crate) mod library;
pub(crate) mod router;
pub(crate) mod service;
pub(crate) mod worker;

// TODO: add injected token into struct
#[derive(Debug)]
pub(crate) struct ChangeCommand {
    application: ApplicationCommand,
    library: LibraryCommand,
    service: ServiceCommand,
    router: RouterCommand,
    worker: WorkerCommand,
}

impl ChangeCommand {
    pub(crate) fn new() -> Self {
        Self {
            application: ApplicationCommand::new(),
            library: LibraryCommand::new(),
            service: ServiceCommand::new(),
            router: RouterCommand::new(),
            worker: WorkerCommand::new(),
        }
    }
}

impl CliCommand for ChangeCommand {
    fn command(&self) -> Command {
        command("change", "Change a forklaunch project")
            .alias("modify")
            .alias("alter")
            .subcommand_required(true)
            .subcommand(CliVersionCommand.command())
            .subcommand(self.application.command())
            .subcommand(self.library.command())
            .subcommand(self.service.command())
            .subcommand(self.router.command())
            .subcommand(self.worker.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("cli-version", sub_matches)) => CliVersionCommand.handler(sub_matches),
            Some(("application", sub_matches)) => self.application.handler(sub_matches),
            Some(("library", sub_matches)) => self.library.handler(sub_matches),
            Some(("service", sub_matches)) => self.service.handler(sub_matches),
            Some(("router", sub_matches)) => self.router.handler(sub_matches),
            Some(("worker", sub_matches)) => self.worker.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
