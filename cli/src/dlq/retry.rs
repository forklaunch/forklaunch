use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct RetryCommand;

impl RetryCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for RetryCommand {
    fn command(&self) -> Command {
        command("retry", "Retry a job from the dead-letter queue").arg(
            Arg::new("job_id")
                .required(true)
                .help("The dead-letter job ID to retry"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let job_id = matches
            .get_one::<String>("job_id")
            .context("job id is required")?;

        let url = format!(
            "{}/dlq/{}/retry",
            get_platform_management_api_url(),
            urlencoding::encode(job_id)
        );
        let response = http_client::post(&url, serde_json::json!({}))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 404 {
            bail!("DLQ job '{}' not found.", job_id);
        }
        if !response.status().is_success() {
            bail!(
                "Failed to retry DLQ job: {}",
                response.text().unwrap_or_default()
            );
        }

        super::print_action_result("Retrying", job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retry_cmd() -> Command {
        RetryCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        retry_cmd().debug_assert();
    }

    #[test]
    fn requires_job_id() {
        assert!(retry_cmd().try_get_matches_from(["retry"]).is_err());
        assert!(retry_cmd().try_get_matches_from(["retry", "job-1"]).is_ok());
    }
}
