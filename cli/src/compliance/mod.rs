use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

pub(crate) mod audit;
pub(crate) mod checks;
mod config;
pub(crate) mod deps;
mod docs;
mod rust_artifact;
pub(crate) mod scan;
mod tenancy;

use audit::AuditCommand;
use config::ConfigCommand;
use docs::DocsCommand;
use tenancy::AuditTenancyCommand;

#[derive(Debug)]
pub(crate) struct ComplianceCommand {
    audit: AuditCommand,
    config: ConfigCommand,
    docs: DocsCommand,
    audit_tenancy: AuditTenancyCommand,
    scan: scan::ScanCommand,
    validate_artifacts: rust_artifact::ValidateArtifactsCommand,
}

impl ComplianceCommand {
    pub(crate) fn new() -> Self {
        Self {
            audit: AuditCommand::new(),
            config: ConfigCommand::new(),
            docs: DocsCommand::new(),
            audit_tenancy: AuditTenancyCommand::new(),
            scan: scan::ScanCommand,
            validate_artifacts: rust_artifact::ValidateArtifactsCommand,
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
        .subcommand(self.config.command())
        .subcommand(self.docs.command())
        .subcommand(self.audit_tenancy.command())
        .subcommand(self.scan.command())
        .subcommand(self.validate_artifacts.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("audit", sub_matches)) => self.audit.handler(sub_matches),
            Some(("config", sub_matches)) => self.config.handler(sub_matches),
            Some(("docs", sub_matches)) => self.docs.handler(sub_matches),
            Some(("scan", sub_matches)) => self.scan.handler(sub_matches),
            Some(("audit-tenancy", sub_matches)) => self.audit_tenancy.handler(sub_matches),
            Some(("validate-artifacts", sub_matches)) => {
                self.validate_artifacts.handler(sub_matches)
            }
            _ => unreachable!(),
        }
    }
}
