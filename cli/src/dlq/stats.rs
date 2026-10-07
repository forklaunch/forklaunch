use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct StatsCommand;

impl StatsCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for StatsCommand {
    fn command(&self) -> Command {
        command("stats", "Show dead-letter queue statistics")
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let json_output = matches.get_flag("json");

        let url = format!("{}/dlq/stats", get_platform_management_api_url());
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to get DLQ stats: {}",
                response.text().unwrap_or_default()
            );
        }
        let stats: DlqStats = response
            .json()
            .with_context(|| "Failed to parse DLQ stats response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&stats)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        writeln!(stdout, "  total       {}", stats.total)?;
        writeln!(stdout, "  waiting     {}", stats.waiting)?;
        writeln!(stdout, "  processed   {}", stats.processed)?;
        writeln!(stdout)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct DlqStats {
    total: u32,
    waiting: u32,
    processed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_cmd() -> Command {
        StatsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        stats_cmd().debug_assert();
    }

    #[test]
    fn dlq_stats_deserializes() {
        let json = r#"{"total": 5, "waiting": 2, "processed": 3}"#;
        let stats: DlqStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.total, 5);
    }
}
