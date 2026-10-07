use anyhow::Result;
use clap::{ArgMatches, Command};
use export::ExportCommand;
use fetch::FetchCommand;

use crate::{CliCommand, core::command::command};

mod export;
mod fetch;

#[derive(Debug)]
pub(crate) struct OpenApiCommand {
    export: ExportCommand,
    fetch: FetchCommand,
}

impl OpenApiCommand {
    pub(crate) fn new() -> Self {
        Self {
            export: ExportCommand::new(),
            fetch: FetchCommand::new(),
        }
    }
}

impl CliCommand for OpenApiCommand {
    fn command(&self) -> Command {
        command("openapi", "OpenAPI specification management")
            .subcommand(self.export.command())
            .subcommand(self.fetch.command())
            .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("export", sub_matches)) => self.export.handler(sub_matches),
            Some(("fetch", sub_matches)) => self.fetch.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
