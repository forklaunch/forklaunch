use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod log_budget;
mod remove_empty;

use log_budget::LogBudgetCommand;
use remove_empty::RemoveEmptyCommand;

/// `org pool` — the organization's compute pool: the EC2 hosts that every
/// `org-shared` application in a region packs onto.
#[derive(Debug)]
pub(super) struct PoolCommand {
    log_budget: LogBudgetCommand,
    remove_empty: RemoveEmptyCommand,
}

impl PoolCommand {
    pub(super) fn new() -> Self {
        Self {
            log_budget: LogBudgetCommand::new(),
            remove_empty: RemoveEmptyCommand::new(),
        }
    }
}

impl CliCommand for PoolCommand {
    fn command(&self) -> Command {
        command(
            "pool",
            "Manage your organization's compute pool: per-host log budget, removing an empty pool",
        )
        .subcommand_required(true)
        .subcommand(self.log_budget.command())
        .subcommand(self.remove_empty.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("log-budget", m)) => self.log_budget.handler(m),
            Some(("remove-empty", m)) => self.remove_empty.handler(m),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool_cmd() -> Command {
        PoolCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        pool_cmd().debug_assert();
    }

    #[test]
    fn a_subcommand_is_required() {
        assert!(pool_cmd().try_get_matches_from(["pool"]).is_err());
    }
}
