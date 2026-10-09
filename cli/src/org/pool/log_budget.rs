//! `forklaunch org pool log-budget` — the per-host log budget of the org pool.
//!
//! Containers on an org compute pool host do not each get a limit of their
//! own: they share one budget of log lines per second per host. This reads
//! that budget, and with `--set` changes it. A change is stored on the pool
//! and takes effect the next time any application on the pool deploys.

use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

/// What the platform applies when a pool has never had a budget set.
const DEFAULT_LINES_PER_SECOND_PER_HOST: u64 = 1000;

#[derive(Debug)]
pub(super) struct LogBudgetCommand;

impl LogBudgetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

/// Response of `GET /compute-pools/log-budget?region=`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BudgetView {
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    pool_exists: bool,
    #[serde(default)]
    log_lines_per_second_per_host: Option<u64>,
}

impl BudgetView {
    fn effective_budget(&self) -> u64 {
        self.log_lines_per_second_per_host
            .unwrap_or(DEFAULT_LINES_PER_SECOND_PER_HOST)
    }
}

/// Response of `PATCH /compute-pools/log-budget`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetBudgetResponse {
    region: String,
    log_lines_per_second_per_host: u64,
    message: String,
}

fn build_set_body(region: &str, lines_per_second_per_host: u64) -> serde_json::Value {
    serde_json::json!({
        "region": region,
        "logLinesPerSecondPerHost": lines_per_second_per_host,
    })
}

fn budget_text(lines_per_second_per_host: u64) -> String {
    format!(
        "{} lines/s per host, shared by every container on the host",
        lines_per_second_per_host
    )
}

fn pool_text(view: &BudgetView) -> &'static str {
    if view.pool_exists {
        "this organization has a pool in this region"
    } else {
        "no pool in this region yet (the budget applies once one is created)"
    }
}

/// 400/403/404 carry a reason meant for a human; relay it as the error.
fn error_from(status: reqwest::StatusCode, detail: String, action: &str) -> anyhow::Error {
    if matches!(status.as_u16(), 400 | 403 | 404) && !detail.trim().is_empty() {
        anyhow::anyhow!("{}", detail.trim())
    } else {
        anyhow::anyhow!("Failed to {} ({}): {}", action, status, detail)
    }
}

impl CliCommand for LogBudgetCommand {
    fn command(&self) -> Command {
        command(
            "log-budget",
            "Show or set the org pool's log budget: lines per second per host, shared by every container on it",
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region of the org compute pool"),
        )
        .arg(
            Arg::new("set")
                .long("set")
                .value_name("N")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help(
                    "New budget in lines per second per host (whole number, at least 1). \
                     Applies the next time any app on the pool deploys",
                ),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Output raw JSON instead of formatted terminal output"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let api_url = get_platform_management_api_url();
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let json_output = matches.get_flag("json");
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        if let Some(budget) = matches.get_one::<u64>("set").copied() {
            let response = http_client::patch(
                &format!("{}/compute-pools/log-budget", api_url),
                build_set_body(region, budget),
            )
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            let status = response.status();
            if !status.is_success() {
                return Err(error_from(
                    status,
                    response.text().unwrap_or_default(),
                    "set the log budget",
                ));
            }
            let result: SetBudgetResponse = response
                .json()
                .with_context(|| "Failed to parse log-budget response")?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            log_ok!(stdout, "{}", result.message);
            writeln!(stdout, "  Region   {}", result.region)?;
            writeln!(
                stdout,
                "  Budget   {}",
                budget_text(result.log_lines_per_second_per_host)
            )?;
            log_info!(
                stdout,
                "The change applies the next time any app on this pool deploys."
            );
            return Ok(());
        }

        let url = format!(
            "{}/compute-pools/log-budget?region={}",
            api_url,
            urlencoding::encode(region)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            return Err(error_from(
                status,
                response.text().unwrap_or_default(),
                "read the org pool",
            ));
        }
        let raw: serde_json::Value = response
            .json()
            .with_context(|| "Failed to parse log budget response")?;
        if json_output {
            println!("{}", serde_json::to_string_pretty(&raw)?);
            return Ok(());
        }
        let view: BudgetView =
            serde_json::from_value(raw).with_context(|| "Failed to parse log budget response")?;

        writeln!(stdout)?;
        log_header!(stdout, Color::Cyan, "Org pool log budget");
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Region   {}",
            view.region.as_deref().unwrap_or(region)
        )?;
        writeln!(
            stdout,
            "  Budget   {}",
            budget_text(view.effective_budget())
        )?;
        writeln!(stdout, "  Pool     {}", pool_text(&view))?;
        writeln!(stdout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        LogBudgetCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn region_is_required() {
        assert!(cmd().try_get_matches_from(["log-budget"]).is_err());
        assert!(
            cmd()
                .try_get_matches_from(["log-budget", "--set", "500"])
                .is_err()
        );
        let m = cmd()
            .try_get_matches_from(["log-budget", "-r", "us-east-1"])
            .unwrap();
        assert!(m.get_one::<u64>("set").is_none());
    }

    #[test]
    fn set_must_be_a_whole_number_of_at_least_one() {
        for bad in ["0", "-5", "2.5", "lots"] {
            assert!(
                cmd()
                    .try_get_matches_from(["log-budget", "-r", "us-east-1", "--set", bad])
                    .is_err(),
                "{bad}"
            );
        }
        let m = cmd()
            .try_get_matches_from(["log-budget", "-r", "us-east-1", "--set", "1"])
            .unwrap();
        assert_eq!(*m.get_one::<u64>("set").unwrap(), 1);
    }

    #[test]
    fn set_body_shape() {
        assert_eq!(
            build_set_body("us-west-2", 2500),
            serde_json::json!({ "region": "us-west-2", "logLinesPerSecondPerHost": 2500 })
        );
    }

    #[test]
    fn view_without_budget_defaults_to_1000() {
        let view: BudgetView =
            serde_json::from_str(r#"{"region":"us-east-1","poolExists":true}"#).unwrap();
        assert_eq!(view.effective_budget(), 1000);
        assert_eq!(
            pool_text(&view),
            "this organization has a pool in this region"
        );
    }

    #[test]
    fn view_with_budget_uses_it() {
        let view: BudgetView = serde_json::from_str(
            r#"{"region":"us-east-1","poolExists":false,"logLinesPerSecondPerHost":4000}"#,
        )
        .unwrap();
        assert_eq!(view.effective_budget(), 4000);
        assert!(pool_text(&view).starts_with("no pool in this region yet"));
        assert_eq!(
            budget_text(4000),
            "4000 lines/s per host, shared by every container on the host"
        );
    }

    #[test]
    fn empty_view_still_parses() {
        let view: BudgetView = serde_json::from_str("{}").unwrap();
        assert_eq!(view.effective_budget(), DEFAULT_LINES_PER_SECOND_PER_HOST);
        assert!(!view.pool_exists);
    }

    #[test]
    fn set_response_deserializes() {
        let r: SetBudgetResponse = serde_json::from_str(
            r#"{"region":"us-east-1","logLinesPerSecondPerHost":2000,"message":"Saved"}"#,
        )
        .unwrap();
        assert_eq!(r.log_lines_per_second_per_host, 2000);
        assert_eq!(r.message, "Saved");
    }

    #[test]
    fn plain_text_errors_are_relayed() {
        let err = error_from(
            reqwest::StatusCode::FORBIDDEN,
            "Only org admins can change the log budget.\n".into(),
            "set the log budget",
        );
        assert_eq!(
            err.to_string(),
            "Only org admins can change the log budget."
        );
        let err = error_from(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "boom".into(),
            "set the log budget",
        );
        assert!(err.to_string().contains("500"), "{err}");
    }
}
