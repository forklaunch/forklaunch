use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use remove::RemoveCommand;
use retry::RetryCommand;
use serde::{Deserialize, Serialize};
use stats::StatsCommand;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

mod remove;
mod retry;
mod stats;

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct DlqCommand {
    retry: RetryCommand,
    remove: RemoveCommand,
    stats: StatsCommand,
}

impl DlqCommand {
    pub(crate) fn new() -> Self {
        Self {
            retry: RetryCommand::new(),
            remove: RemoveCommand::new(),
            stats: StatsCommand::new(),
        }
    }
}

impl CliCommand for DlqCommand {
    fn command(&self) -> Command {
        command(
            "dlq",
            "Inspect and manage the dead-letter queue for failed deployment jobs",
        )
        .arg(
            Arg::new("limit")
                .long("limit")
                .help("Max jobs to list (default 100)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .subcommand(self.retry.command())
        .subcommand(self.remove.command())
        .subcommand(self.stats.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("retry", sub_matches)) => self.retry.handler(sub_matches),
            Some(("remove", sub_matches)) => self.remove.handler(sub_matches),
            Some(("stats", sub_matches)) => self.stats.handler(sub_matches),
            _ => list_jobs(matches),
        }
    }
}

fn print_action_result(verb: &str, job_id: &str) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
    write!(stdout, "  {}", verb)?;
    stdout.reset()?;
    writeln!(stdout, "  DLQ job {}", job_id)?;
    Ok(())
}

// ── List handler ──────────────────────────────────────────────────────────────

fn list_jobs(matches: &ArgMatches) -> Result<()> {
    let _token = require_auth()?;
    let limit = matches.get_one::<String>("limit").cloned();
    let json_output = matches.get_flag("json");

    let mut url = format!("{}/dlq", get_platform_management_api_url());
    if let Some(l) = &limit {
        url.push_str(&format!("?limit={}", urlencoding::encode(l)));
    }

    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        bail!(
            "Failed to list DLQ jobs: {}",
            response.text().unwrap_or_default()
        );
    }
    // The list endpoint responds with a bare JSON array (schemaValidator.array(...)),
    // not a {jobs: [...]} wrapper.
    let jobs: Vec<DlqJob> = response
        .json()
        .with_context(|| "Failed to parse DLQ jobs response")?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&jobs)?);
        return Ok(());
    }

    print_jobs(&jobs)
}

fn print_jobs(jobs: &[DlqJob]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if jobs.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        writeln!(stdout, "No jobs in the dead-letter queue.")?;
        stdout.reset()?;
        return Ok(());
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<36}  {:<10}  {:<20}  {}",
        "ID", "ATTEMPTS", "FAILED AT", "ERROR"
    )?;
    stdout.reset()?;

    for job in jobs {
        let error_truncated = if job.error.chars().count() > 50 {
            format!("{}…", job.error.chars().take(49).collect::<String>())
        } else {
            job.error.clone()
        };
        writeln!(
            stdout,
            "  {:<36}  {:<10}  {:<20}  {}",
            job.id,
            job.attempts,
            job.failed_at.get(..19).unwrap_or(&job.failed_at),
            error_truncated
        )?;
    }

    writeln!(stdout)?;
    writeln!(stdout, "  {} job(s) in the dead-letter queue.", jobs.len())?;
    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DlqJob {
    id: String,
    deployment_id: String,
    application_id: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    region: Option<String>,
    error: String,
    attempts: u32,
    failed_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dlq_cmd() -> Command {
        DlqCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        dlq_cmd().debug_assert();
    }

    #[test]
    fn list_has_no_required_args() {
        assert!(dlq_cmd().try_get_matches_from(["dlq"]).is_ok());
    }

    #[test]
    fn dlq_job_deserializes() {
        let json = r#"{
            "id": "job-1",
            "deploymentId": "dep-1",
            "applicationId": "app-1",
            "error": "timeout",
            "attempts": 3,
            "failedAt": "2024-01-15T10:00:00Z"
        }"#;
        let job: DlqJob = serde_json::from_str(json).unwrap();
        assert_eq!(job.id, "job-1");
        assert_eq!(job.attempts, 3);
    }

    #[test]
    fn dlq_jobs_list_deserializes_bare_array() {
        let json = r#"[{"id":"job-1","deploymentId":"d","applicationId":"a","error":"e","attempts":1,"failedAt":"2024-01-15T10:00:00Z"}]"#;
        let jobs: Vec<DlqJob> = serde_json::from_str(json).unwrap();
        assert_eq!(jobs.len(), 1);
    }
}
