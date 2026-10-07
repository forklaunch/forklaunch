use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct UnlinkCommand;

impl UnlinkCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for UnlinkCommand {
    fn command(&self) -> Command {
        command("unlink", "Unlink the BYOC cloud account").arg(
            Arg::new("id")
                .required(true)
                .help("The cloud account id to unlink"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches.get_one::<String>("id").context("id is required")?;

        let url = format!("{}/cloud-accounts/{}", get_platform_management_api_url(), urlencoding::encode(id));
        let response = http_client::delete(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to unlink cloud account: {}",
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "  Unlinked")?;
        stdout.reset()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlink_cmd() -> Command {
        UnlinkCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        unlink_cmd().debug_assert();
    }

    #[test]
    fn requires_id() {
        assert!(unlink_cmd().try_get_matches_from(["unlink"]).is_err());
        assert!(unlink_cmd().try_get_matches_from(["unlink", "ca-1"]).is_ok());
    }
}
