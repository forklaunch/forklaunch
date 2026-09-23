use anyhow::Result;
use clap::{ArgMatches, Command};
use connect::ConnectCommand;
use disconnect::DisconnectCommand;
use install::InstallCommand;
use status::StatusCommand;

use crate::{
    CliCommand, constants::get_platform_management_api_url, core::command::command,
};

mod connect;
mod disconnect;
mod install;
mod status;

/// URL of an org-level GitHub route, e.g. `github_app_url("status")`.
pub(super) fn github_app_url(action: &str) -> String {
    format!("{}/github-app/{}", get_platform_management_api_url(), action)
}

/// URL of an application-scoped GitHub route.
///
/// Every GitHub route on the platform — the org-level ones and the
/// application-scoped ones alike — is mounted under the `/github-app` router.
/// `install` and `status` spelled that prefix out; `connect`, `disconnect` and
/// the settings read in `status` did not, and posted to `/applications/...`
/// instead. The platform answered 404, `connect` reported it as a failure to
/// connect the repository, and `status` swallowed it and printed nothing —
/// so a connected repository and a 404 looked identical.
///
/// Build these URLs here so the prefix cannot be dropped again.
pub(super) fn application_github_url(application_id: &str, action: &str) -> String {
    format!(
        "{}/github-app/applications/{}/github/{}",
        get_platform_management_api_url(),
        application_id,
        action
    )
}

#[derive(Debug)]
pub(crate) struct GithubCommand {
    connect: ConnectCommand,
    disconnect: DisconnectCommand,
    install: InstallCommand,
    status: StatusCommand,
}

impl GithubCommand {
    pub(crate) fn new() -> Self {
        Self {
            connect: ConnectCommand::new(),
            disconnect: DisconnectCommand::new(),
            install: InstallCommand::new(),
            status: StatusCommand::new(),
        }
    }
}

impl CliCommand for GithubCommand {
    fn command(&self) -> Command {
        command(
            "github",
            "Connect GitHub repositories and configure autodeploy",
        )
        .subcommand(self.install.command())
        .subcommand(self.status.command())
        .subcommand(self.connect.command())
        .subcommand(self.disconnect.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("install", sub_matches)) => self.install.handler(sub_matches),
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("connect", sub_matches)) => self.connect.handler(sub_matches),
            Some(("disconnect", sub_matches)) => self.disconnect.handler(sub_matches),
            _ => {
                GithubCommand::new().command().print_help()?;
                Ok(())
            }
        }
    }
}
