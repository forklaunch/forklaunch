use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRollbackResponse {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    rollback_id: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RollbackCommand;

impl RollbackCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for RollbackCommand {
    fn command(&self) -> Command {
        command("rollback", "Roll back a deployment to a previous release")
            .arg(
                Arg::new("deployment_id")
                    .long("deployment")
                    .short('d')
                    .required(true)
                    .help("The deployment id to roll back"),
            )
            .arg(
                Arg::new("target_release_id")
                    .long("target-release")
                    .short('t')
                    .required(true)
                    .help("The release id to roll back to"),
            )
            .arg(
                Arg::new("reason")
                    .long("reason")
                    .help("Optional reason for the rollback, recorded on the rollback record"),
            )
            .arg(
                Arg::new("priority")
                    .long("priority")
                    .help("Optional priority (higher runs sooner if multiple rollbacks queue)"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let deployment_id = matches
            .get_one::<String>("deployment_id")
            .context("--deployment is required")?;
        let target_release_id = matches
            .get_one::<String>("target_release_id")
            .context("--target-release is required")?;
        let reason = matches.get_one::<String>("reason");
        let priority: Option<i64> = matches
            .get_one::<String>("priority")
            .map(|p| p.parse())
            .transpose()
            .context("--priority must be an integer")?;

        let mut body = serde_json::json!({
            "deploymentId": deployment_id,
            "targetReleaseId": target_release_id,
        });
        if let Some(r) = reason {
            body["reason"] = serde_json::Value::String(r.clone());
        }
        if let Some(p) = priority {
            body["priority"] = serde_json::Value::from(p);
        }

        let url = format!("{}/rollbacks", get_platform_management_api_url());
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to create rollback: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: CreateRollbackResponse = response
            .json()
            .with_context(|| "Failed to parse rollback response")?;

        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Rollback queued")?;
        stdout.reset()?;
        if let Some(id) = &result.rollback_id {
            writeln!(stdout, "  ({})", id)?;
        } else {
            writeln!(stdout)?;
        }
        if let Some(msg) = &result.message {
            writeln!(stdout, "  {}", msg)?;
        }
        writeln!(stdout)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollback_cmd() -> Command {
        RollbackCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        rollback_cmd().debug_assert();
    }

    #[test]
    fn requires_deployment_and_target_release() {
        assert!(rollback_cmd().try_get_matches_from(["rollback"]).is_err());
        assert!(
            rollback_cmd()
                .try_get_matches_from(["rollback", "-d", "dep-1"])
                .is_err()
        );
        assert!(
            rollback_cmd()
                .try_get_matches_from(["rollback", "-d", "dep-1", "-t", "rel-1"])
                .is_ok()
        );
    }

    #[test]
    fn rejects_non_numeric_priority() {
        let matches = rollback_cmd()
            .try_get_matches_from([
                "rollback",
                "-d",
                "dep-1",
                "-t",
                "rel-1",
                "--priority",
                "not-a-number",
            ])
            .unwrap();
        let parsed: Result<i64, _> = matches.get_one::<String>("priority").unwrap().parse();
        assert!(parsed.is_err());
    }
}
