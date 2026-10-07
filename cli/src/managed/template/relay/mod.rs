use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod clear;
mod list;
mod set;

use clear::ClearCommand;
use list::ListCommand;
use set::SetCommand;

#[derive(Debug)]
pub(super) struct RelayCommand {
    list: ListCommand,
    set: SetCommand,
    clear: ClearCommand,
}

impl RelayCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand::new(),
            set: SetCommand::new(),
            clear: ClearCommand::new(),
        }
    }
}

impl CliCommand for RelayCommand {
    fn command(&self) -> Command {
        command(
            "relay",
            "Declare where a provider's callback (Epic, Google, a webhook) is handed on each instance",
        )
        .long_about(
            "Declare where a provider's callback is handed on each instance.\n\n\
             A provider such as Epic lets you register ONE redirect URI per app, but a\n\
             product has many instances. So every instance sends the provider to one\n\
             platform-owned address — the product's RELAY (`list` prints it) — and the\n\
             platform works out which instance the callback belongs to and hands it on.\n\n\
             A ROUTE says where: which component of the instance, at which path, and how.\n\
             \x20 redirect   the browser is sent to https://<prefix>-<component>.<zone><path>\n\
             \x20            with the provider's query intact, and the instance finishes the\n\
             \x20            sign-in with its own keys (browser OAuth; what Epic with PKCE needs)\n\
             \x20 forward    the callback is POSTed to the component over the internal mesh,\n\
             \x20            signed with the instance's key (webhooks)\n\n\
             The route named `default` answers at the bare .../callback, so a redirect URI\n\
             registered before routes existed keeps working; any other name answers at\n\
             .../callback/<name>, and `list` shows the URL to register for each.\n\n\
             Publishing a version checks the routes: the named component must exist in the\n\
             build and serve the path (GET for redirect; POST with internal auth for forward).",
        )
        .subcommand(self.list.command())
        .subcommand(self.set.command())
        .subcommand(self.clear.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("set", sub_matches)) => self.set.handler(sub_matches),
            Some(("clear", sub_matches)) => self.clear.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}
