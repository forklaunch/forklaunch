use anyhow::Result;
use approval::ApprovalCommand;
use branch::BranchCommand;
use clap::{ArgMatches, Command};
use create::CreateCommand;
use delete::DeleteCommand;
use status::StatusCommand;
use sync::SyncCommand;
use validate::ValidateCommand;

use crate::{CliCommand, core::command::command};

pub(crate) mod approval;
pub(crate) mod branch;
pub(crate) mod create;
pub(crate) mod delete;
pub(crate) mod status;
pub(crate) mod sync;
pub(crate) mod validate;

#[derive(Debug)]
pub(crate) struct EnvironmentCommand {
    validate: ValidateCommand,
    sync: SyncCommand,
    status: StatusCommand,
    create: CreateCommand,
    delete: DeleteCommand,
    branch: BranchCommand,
    approval: ApprovalCommand,
}

impl EnvironmentCommand {
    pub(crate) fn new() -> Self {
        Self {
            validate: ValidateCommand::new(),
            sync: SyncCommand::new(),
            status: StatusCommand::new(),
            create: CreateCommand::new(),
            delete: DeleteCommand::new(),
            branch: BranchCommand::new(),
            approval: ApprovalCommand::new(),
        }
    }
}

impl CliCommand for EnvironmentCommand {
    fn command(&self) -> Command {
        command(
            "environment",
            "Create, delete and configure environments, and their variables",
        )
        .alias("env")
        .subcommand_required(true)
        .subcommand(self.validate.command())
        .subcommand(self.sync.command())
        .subcommand(self.status.command())
        .subcommand(self.create.command())
        .subcommand(self.delete.command())
        .subcommand(self.branch.command())
        .subcommand(self.approval.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("validate", sub_matches)) => self.validate.handler(sub_matches),
            Some(("sync", sub_matches)) => self.sync.handler(sub_matches),
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("delete", sub_matches)) => self.delete.handler(sub_matches),
            Some(("branch", sub_matches)) => self.branch.handler(sub_matches),
            Some(("approval", sub_matches)) => self.approval.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
