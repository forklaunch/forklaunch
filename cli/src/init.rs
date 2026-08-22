use anyhow::Result;
use application::ApplicationCommand;
use clap::{ArgMatches, Command};
use library::LibraryCommand;
use module::ModuleCommand;
use router::RouterCommand;
use service::ServiceCommand;
use worker::WorkerCommand;

use crate::{CliCommand, core::command::command};

pub(crate) mod application;
pub(crate) mod library;
pub(crate) mod module;
pub(crate) mod router;
pub(crate) mod service;
pub(crate) mod storefront;
pub(crate) mod worker;

// TODO: add injected token into struct
#[derive(Debug)]
pub(crate) struct InitCommand {
    application: ApplicationCommand,
    library: LibraryCommand,
    module: ModuleCommand,
    router: RouterCommand,
    service: ServiceCommand,
    worker: WorkerCommand,
}

impl InitCommand {
    pub(crate) fn new() -> Self {
        Self {
            application: ApplicationCommand::new(),
            library: LibraryCommand::new(),
            module: ModuleCommand::new(),
            router: RouterCommand::new(),
            service: ServiceCommand::new(),
            worker: WorkerCommand::new(),
        }
    }
}

impl CliCommand for InitCommand {
    fn command(&self) -> Command {
        command("init", "Initialize a new forklaunch project")
            .alias("add")
            .subcommand_required(true)
            .subcommand(self.application.command())
            .subcommand(self.library.command())
            .subcommand(self.module.command())
            .subcommand(self.router.command())
            .subcommand(self.service.command())
            .subcommand(self.worker.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("application", sub_matches)) => self.application.handler(sub_matches),
            Some(("library", sub_matches)) => self.library.handler(sub_matches),
            Some(("module", sub_matches)) => self.module.handler(sub_matches),
            Some(("service", sub_matches)) => self.service.handler(sub_matches),
            Some(("router", sub_matches)) => self.router.handler(sub_matches),
            Some(("worker", sub_matches)) => self.worker.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
