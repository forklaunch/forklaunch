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
pub(crate) struct LinkCommand;

impl LinkCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for LinkCommand {
    fn command(&self) -> Command {
        command(
            "link",
            "Complete a cloud account link by providing the IAM role ARN",
        )
        .arg(
            Arg::new("id")
                .required(true)
                .help("The cloud account id (from `fl cloud-account create`)"),
        )
        .arg(
            Arg::new("role_arn")
                .long("role-arn")
                .required(true)
                .help("The IAM role ARN created in your AWS account"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches.get_one::<String>("id").context("id is required")?;
        let role_arn = matches
            .get_one::<String>("role_arn")
            .context("--role-arn is required")?;

        let url = format!("{}/cloud-accounts/{}", get_platform_management_api_url(), urlencoding::encode(id));
        let body = serde_json::json!({ "roleArn": role_arn });
        let response =
            http_client::put(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to link cloud account: {}",
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "  Linked")?;
        stdout.reset()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_cmd() -> Command {
        LinkCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        link_cmd().debug_assert();
    }

    #[test]
    fn requires_id_and_role_arn() {
        assert!(link_cmd().try_get_matches_from(["link"]).is_err());
        assert!(
            link_cmd()
                .try_get_matches_from(["link", "ca-1", "--role-arn", "arn:aws:iam::123:role/x"])
                .is_ok()
        );
    }
}
