use anyhow::Result;
use check::CheckCommand;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod check;

#[derive(Debug)]
pub(crate) struct DriftCommand {
    check: CheckCommand,
}

impl DriftCommand {
    pub(crate) fn new() -> Self {
        Self {
            check: CheckCommand::new(),
        }
    }
}

impl CliCommand for DriftCommand {
    fn command(&self) -> Command {
        command(
            "drift",
            "Detect hosting-configuration drift — services/workers whose hosting type no longer matches the plan's allowlist",
        )
        .subcommand_required(true)
        .subcommand(self.check.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("check", sub_matches)) => self.check.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drift_cmd() -> Command {
        DriftCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        drift_cmd().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(drift_cmd().try_get_matches_from(["drift"]).is_err());
        assert!(drift_cmd().try_get_matches_from(["drift", "check"]).is_ok());
    }
}
