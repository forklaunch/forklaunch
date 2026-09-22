use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod audit;
pub(crate) mod checks;
mod tenancy;

use audit::AuditCommand;
use tenancy::AuditTenancyCommand;

#[derive(Debug)]
pub(crate) struct ComplianceCommand {
    audit: AuditCommand,
    audit_tenancy: AuditTenancyCommand,
}

impl ComplianceCommand {
    pub(crate) fn new() -> Self {
        Self {
            audit: AuditCommand::new(),
            audit_tenancy: AuditTenancyCommand::new(),
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
        .subcommand(self.audit_tenancy.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("audit", sub_matches)) => self.audit.handler(sub_matches),
            Some(("audit-tenancy", sub_matches)) => self.audit_tenancy.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
