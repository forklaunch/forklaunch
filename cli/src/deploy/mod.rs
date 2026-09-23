use anyhow::Result;
use approvals::ApprovalsCommand;
use cancel::CancelCommand;
use clap::{ArgMatches, Command};
use create::CreateCommand;
use destroy::DestroyCommand;
use info::InfoCommand;
use logs::LogsCommand;
use rollback::RollbackCommand;

use crate::{CliCommand, core::command::command};

mod approvals;
mod cancel;
mod create;
mod destroy;
mod info;
mod logs;
mod rollback;
mod target;
pub(crate) mod utils;

#[derive(Debug)]
pub(crate) struct DeployCommand {
    approvals: ApprovalsCommand,
    cancel: CancelCommand,
    create: CreateCommand,
    destroy: DestroyCommand,
    info: InfoCommand,
    logs: LogsCommand,
    rollback: RollbackCommand,
}

impl DeployCommand {
    pub(crate) fn new() -> Self {
        Self {
            approvals: ApprovalsCommand::new(),
            cancel: CancelCommand::new(),
            create: CreateCommand::new(),
            destroy: DestroyCommand::new(),
            info: InfoCommand::new(),
            logs: LogsCommand::new(),
            rollback: RollbackCommand::new(),
        }
    }
}

impl CliCommand for DeployCommand {
    fn command(&self) -> Command {
        command("deploy", "Deployment management")
            .subcommand(self.approvals.command())
            .subcommand(self.cancel.command())
            .subcommand(self.create.command())
            .subcommand(self.destroy.command())
            .subcommand(self.info.command())
            .subcommand(self.logs.command())
            .subcommand(self.rollback.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("approvals", sub_matches)) => self.approvals.handler(sub_matches),
            Some(("cancel", sub_matches)) => self.cancel.handler(sub_matches),
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("destroy", sub_matches)) => self.destroy.handler(sub_matches),
            Some(("info", sub_matches)) => self.info.handler(sub_matches),
            Some(("logs", sub_matches)) => self.logs.handler(sub_matches),
            Some(("rollback", sub_matches)) => self.rollback.handler(sub_matches),
            // Default to create for convenience - preserving existing behavior but usually nice to be explicit
            None => self.create.handler(matches),
            _ => unreachable!(),
        }
    }
}
