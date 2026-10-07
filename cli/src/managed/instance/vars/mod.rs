use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod list;
mod set;
mod unset;

use list::ListCommand;
use set::SetCommand;
use unset::UnsetCommand;

#[derive(Debug)]
pub(super) struct VarsCommand {
    list: ListCommand,
    set: SetCommand,
    unset: UnsetCommand,
}

impl VarsCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand::new(),
            set: SetCommand::new(),
            unset: UnsetCommand::new(),
        }
    }
}

impl CliCommand for VarsCommand {
    fn command(&self) -> Command {
        command("vars", "Fill in one instance's custom variable values")
            .long_about(
                "Fill in one instance's custom variable values.\n\n\
                 The TEMPLATE decides which variables exist; this command only supplies values\n\
                 for the `custom` ones — the variables whose value differs per customer, like\n\
                 that customer's own Stripe key. You cannot add a variable here that the\n\
                 template did not declare; declare it with `managed template vars set` first.\n\n\
                 The other two kinds need nothing here. A `static` variable's value lives on\n\
                 the template and is the same for everyone. A `generated` variable's value is\n\
                 derived by the instance itself.\n\n\
                 `list` is the command to run before launching: it shows every variable the\n\
                 template declares and, for the custom ones, whether this instance has a value\n\
                 yet. A variable that is both REQUIRED and MISSING will stop the instance from\n\
                 being provisioned.\n\n\
                 Values are never printed back. `list` reports SET or MISSING, not the value —\n\
                 there is no command here that reads a secret back out.",
            )
            .subcommand(self.list.command())
            .subcommand(self.set.command())
            .subcommand(self.unset.command())
            .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("set", sub_matches)) => self.set.handler(sub_matches),
            Some(("unset", sub_matches)) => self.unset.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
