use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, resolve_auth},
    },
};

/// `deploy cancel <id>` — you could start a deployment from the CLI and had to
/// open the dashboard to stop one.
#[derive(Debug)]
pub(crate) struct CancelCommand;

impl CancelCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CancelCommand {
    fn command(&self) -> Command {
        command("cancel", "Cancel a running deployment")
            .long_about(
                "Cancel a running deployment by id.\n\n\
                 Cancellation is cooperative: the deployment stops at its next checkpoint, \
                 so a `pulumi up` already in flight finishes that step first. Follow it with \
                 `forklaunch deploy info --deployment <id>`.",
            )
            .arg(
                Arg::new("deployment")
                    .required(true)
                    .help("Deployment id to cancel"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let auth_mode = resolve_auth()?;
        let deployment_id = matches
            .get_one::<String>("deployment")
            .context("deployment id is required")?;

        let url = format!(
            "{}/deployments/{}/cancel",
            get_platform_management_api_url(),
            urlencoding::encode(deployment_id)
        );
        let response = http_client::post_with_auth(&auth_mode, &url, serde_json::json!({}))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().unwrap_or_default();
            if status.as_u16() == 400 {
                bail!(
                    "Deployment {} cannot be cancelled ({}): {}",
                    deployment_id,
                    status,
                    detail
                );
            }
            bail!("Failed to cancel deployment ({}): {}", status, detail);
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)).set_bold(true))?;
        write!(stdout, "  Cancelling")?;
        stdout.reset()?;
        writeln!(stdout, "  deployment {}", deployment_id)?;
        writeln!(
            stdout,
            "  It stops at its next checkpoint. Follow it: forklaunch deploy info --deployment {}",
            deployment_id
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        CancelCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn requires_a_deployment_id() {
        assert!(cmd().try_get_matches_from(["cancel"]).is_err());
        assert!(cmd().try_get_matches_from(["cancel", "dep-1"]).is_ok());
    }
}
