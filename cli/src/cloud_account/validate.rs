use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct ValidateCommand;

impl ValidateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ValidateCommand {
    fn command(&self) -> Command {
        command("validate", "Re-validate an existing cloud account link").arg(
            Arg::new("id")
                .required(true)
                .help("The cloud account id to re-validate"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches.get_one::<String>("id").context("id is required")?;

        let url = format!(
            "{}/cloud-accounts/{}/validate",
            get_platform_management_api_url(),
            urlencoding::encode(id)
        );
        let response = http_client::post(&url, serde_json::json!({}))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to validate cloud account: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: ValidateResponse = response
            .json()
            .with_context(|| "Failed to parse validate response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout, "  status: {}", result.status)?;
        if let Some(err) = &result.validation_error {
            writeln!(stdout, "  error:  {}", err)?;
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateResponse {
    status: String,
    #[serde(default)]
    validation_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_cmd() -> Command {
        ValidateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        validate_cmd().debug_assert();
    }

    #[test]
    fn requires_id() {
        assert!(validate_cmd().try_get_matches_from(["validate"]).is_err());
        assert!(
            validate_cmd()
                .try_get_matches_from(["validate", "ca-1"])
                .is_ok()
        );
    }
}
