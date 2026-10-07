use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct RemoveCommand;

impl RemoveCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for RemoveCommand {
    fn command(&self) -> Command {
        command("remove", "Permanently remove a job from the dead-letter queue").arg(
            Arg::new("job_id")
                .required(true)
                .help("The dead-letter job ID to remove"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let job_id = matches
            .get_one::<String>("job_id")
            .context("job id is required")?;

        let url = format!("{}/dlq/{}", get_platform_management_api_url(), urlencoding::encode(job_id));
        let response = http_client::delete(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 404 {
            bail!("DLQ job '{}' not found.", job_id);
        }
        if !response.status().is_success() {
            bail!(
                "Failed to remove DLQ job: {}",
                response.text().unwrap_or_default()
            );
        }

        super::print_action_result("Removed", job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove_cmd() -> Command {
        RemoveCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        remove_cmd().debug_assert();
    }

    #[test]
    fn requires_job_id() {
        assert!(remove_cmd().try_get_matches_from(["remove"]).is_err());
        assert!(
            remove_cmd()
                .try_get_matches_from(["remove", "job-1"])
                .is_ok()
        );
    }
}
