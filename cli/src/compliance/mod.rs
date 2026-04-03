use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod audit;

use audit::AuditCommand;

#[derive(Debug)]
pub(crate) struct ComplianceCommand {
    audit: AuditCommand,
}

impl ComplianceCommand {
    pub(crate) fn new() -> Self {
        Self {
            audit: AuditCommand::new(),
        }
    }
}

impl CliCommand for ComplianceCommand {
    fn command(&self) -> Command {
        command(
            "compliance",
            "Compliance management and audit reporting. More info: https://forklaunch.com/docs/compliance",
        )
        .subcommand(self.audit.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("audit", sub_matches)) => self.audit.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
