use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod domain;
mod invitations;
mod members;
mod shared;
mod show;

use domain::DomainCommand;
use invitations::InvitationsCommand;
use members::MembersCommand;
use show::ShowCommand;

/// `org` — organization administration, which was dashboard-only in full.
///
/// Onboarding a teammate, changing someone's role, or turning on domain
/// auto-join meant opening the dashboard, so none of it could be scripted or
/// handed to an agent. Everything here is scoped to the caller's own
/// organization: there is no `--organization` flag, because the session's
/// organization is the only one these routes act on.
#[derive(Debug)]
pub(crate) struct OrgCommand {
    show: ShowCommand,
    members: MembersCommand,
    invitations: InvitationsCommand,
    domain: DomainCommand,
}

impl OrgCommand {
    pub(crate) fn new() -> Self {
        Self {
            show: ShowCommand::new(),
            members: MembersCommand::new(),
            invitations: InvitationsCommand::new(),
            domain: DomainCommand::new(),
        }
    }
}

impl CliCommand for OrgCommand {
    fn command(&self) -> Command {
        command(
            "org",
            "Show and administer your organization: members, invitations, domain",
        )
        .subcommand(self.show.command())
        .subcommand(self.members.command())
        .subcommand(self.invitations.command())
        .subcommand(self.domain.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("show", m)) => self.show.handler(m),
            Some(("members", m)) => self.members.handler(m),
            Some(("invitations", m)) => self.invitations.handler(m),
            Some(("domain", m)) => self.domain.handler(m),
            _ => self.show.handler(matches),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        OrgCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }
}
