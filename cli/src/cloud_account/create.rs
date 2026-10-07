use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command(
            "create",
            "Start linking a BYOC cloud account — returns setup instructions to run in your own AWS account",
        )
        .arg(
            Arg::new("provider")
                .long("provider")
                .default_value("aws")
                .help("Cloud provider (currently only aws)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let provider = matches
            .get_one::<String>("provider")
            .map(String::as_str)
            .unwrap_or("aws");

        let url = format!("{}/cloud-accounts", get_platform_management_api_url());
        let body = serde_json::json!({ "provider": provider });
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to create cloud account: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: CloudAccountWithSetup = response
            .json()
            .with_context(|| "Failed to parse cloud account response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "  Cloud account created ({})", result.account.id)?;
        stdout.reset()?;
        writeln!(stdout)?;
        writeln!(stdout, "  CloudFormation: {}", result.setup_instructions.cloud_formation_url)?;
        writeln!(stdout)?;
        writeln!(stdout, "  Terraform:")?;
        writeln!(stdout, "{}", result.setup_instructions.terraform_snippet)?;
        writeln!(stdout)?;
        writeln!(stdout, "  Manual steps:")?;
        for (i, step) in result.setup_instructions.manual_steps.iter().enumerate() {
            writeln!(stdout, "    {}. {}", i + 1, step)?;
        }
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Once done, run `fl cloud-account link {} --role-arn <arn>` to complete setup.",
            result.account.id
        )?;
        writeln!(stdout)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInstructions {
    cloud_formation_url: String,
    terraform_snippet: String,
    manual_steps: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AccountSummary {
    id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudAccountWithSetup {
    account: AccountSummary,
    setup_instructions: SetupInstructions,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_cmd() -> Command {
        CreateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        create_cmd().debug_assert();
    }

    #[test]
    fn cloud_account_with_setup_deserializes() {
        let json = r#"{
            "account": {"id": "ca-1"},
            "setupInstructions": {
                "cloudFormationUrl": "https://...",
                "terraformSnippet": "resource ...",
                "manualSteps": ["step 1", "step 2"]
            }
        }"#;
        let result: CloudAccountWithSetup = serde_json::from_str(json).unwrap();
        assert_eq!(result.account.id, "ca-1");
        assert_eq!(result.setup_instructions.manual_steps.len(), 2);
    }
}
