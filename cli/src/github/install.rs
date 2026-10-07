use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallUrlResponse {
    install_url: String,
}

#[derive(Debug)]
pub(crate) struct InstallCommand;

impl InstallCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for InstallCommand {
    fn command(&self) -> Command {
        command(
            "install",
            "Get the GitHub App installation link for your organization",
        )
    }

    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        require_auth()?;

        let url = format!(
            "{}/github-app/install-url",
            get_platform_management_api_url()
        );
        let response = http_client::get(&url)?;
        if !response.status().is_success() {
            bail!(
                "Failed to fetch GitHub App install URL (Status: {})",
                response.status()
            );
        }
        let data: InstallUrlResponse = response
            .json()
            .with_context(|| "Failed to parse install URL response")?;

        log_header!(stdout, Color::Yellow, "Install the ForkLaunch GitHub App:");
        log_header!(stdout, Color::Yellow, "{}", data.install_url);
        log_info!(
            stdout,
            "Open the link, choose the GitHub org that owns your repositories, and grant access. Then run `forklaunch github status` to confirm."
        );

        Ok(())
    }
}
