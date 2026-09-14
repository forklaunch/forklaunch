use anyhow::Result;
use clap::{ArgMatches, Command};
use controller::ControllerCommand;
use create::CreateCommand;
use domain::DomainCommand;
use hosting::HostingCommand;
use resize::ResizeCommand;
use route::RouteCommand;
use services::ServicesCommand;

use crate::{CliCommand, core::command::command};

mod controller;
mod create;
mod domain;
mod hosting;
mod resize;
mod route;
mod services;

#[derive(Debug)]
pub(crate) struct AppCommand {
    create: CreateCommand,
    services: ServicesCommand,
    domain: DomainCommand,
    hosting: HostingCommand,
    resize: ResizeCommand,
    route: RouteCommand,
    controller: ControllerCommand,
}

impl AppCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            services: ServicesCommand::new(),
            domain: DomainCommand::new(),
            hosting: HostingCommand::new(),
            resize: ResizeCommand::new(),
            route: RouteCommand::new(),
            controller: ControllerCommand::new(),
        }
    }
}

impl CliCommand for AppCommand {
    fn command(&self) -> Command {
        command("app", "Manage platform applications")
            .subcommand_required(true)
            .subcommand(self.create.command())
            .subcommand(self.services.command())
            .subcommand(self.domain.command())
            .subcommand(self.hosting.command())
            .subcommand(self.resize.command())
            .subcommand(self.route.command())
            .subcommand(self.controller.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", matches)) => self.create.handler(matches),
            Some(("services", matches)) => self.services.handler(matches),
            Some(("domain", matches)) => self.domain.handler(matches),
            Some(("hosting", matches)) => self.hosting.handler(matches),
            Some(("resize", matches)) => self.resize.handler(matches),
            Some(("route", matches)) => self.route.handler(matches),
            Some(("controller", matches)) => self.controller.handler(matches),
            _ => unreachable!(),
        }
    }
}
