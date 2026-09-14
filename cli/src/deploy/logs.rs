use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// Default number of trailing lines shown. A deployment log runs to tens of
/// thousands of lines; the tail is almost always the part someone wants.
const DEFAULT_TAIL: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    message: String,
    #[serde(default)]
    step: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentLogsResponse {
    #[serde(default)]
    logs: Vec<LogEntry>,
    #[serde(default)]
    total_lines: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentSummary {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct DeploymentListResponse {
    #[serde(default)]
    deployments: Vec<DeploymentSummary>,
}

fn level_color(level: &str) -> Option<Color> {
    match level.to_ascii_lowercase().as_str() {
        "error" => Some(Color::Red),
        "warn" | "warning" => Some(Color::Yellow),
        "debug" => Some(Color::Cyan),
        _ => None,
    }
}

/// Trim the ISO timestamp down to `HH:MM:SS`. The date is the same for every
/// line of a single deployment, and repeating it 200 times crowds out the
/// message.
fn short_time(timestamp: &Option<String>) -> String {
    let Some(ts) = timestamp else {
        return " ".repeat(8);
    };
    ts.split('T')
        .nth(1)
        .and_then(|time| time.get(0..8))
        .map(str::to_string)
        .unwrap_or_else(|| " ".repeat(8))
}

fn print_entry(out: &mut StandardStream, entry: &LogEntry) -> Result<()> {
    let level = entry.level.clone().unwrap_or_else(|| "info".to_string());

    out.set_color(ColorSpec::new().set_dimmed(true))?;
    write!(out, "{} ", short_time(&entry.timestamp))?;
    out.reset()?;

    let mut level_spec = ColorSpec::new();
    if let Some(color) = level_color(&level) {
        level_spec.set_fg(Some(color)).set_bold(true);
    } else {
        level_spec.set_dimmed(true);
    }
    out.set_color(&level_spec)?;
    write!(out, "{:<5} ", level.to_ascii_uppercase())?;
    out.reset()?;

    if let Some(step) = &entry.step {
        out.set_color(ColorSpec::new().set_dimmed(true))?;
        write!(out, "[{}] ", step)?;
        out.reset()?;
    }

    writeln!(out, "{}", entry.message)?;
    Ok(())
}

/// The build's output is streamed into the deployment log while it runs, but
/// that stream is best-effort — when it drops, this is the pointer to the
/// authoritative copy in the account the deployment ran in.
fn print_build_log_location(
    out: &mut StandardStream,
    metadata: &Option<serde_json::Value>,
) -> Result<()> {
    let Some(build) = metadata
        .as_ref()
        .and_then(|m| m.get("codeBuildLogs"))
        .and_then(|b| b.as_object())
    else {
        return Ok(());
    };
    let Some(group) = build.get("logGroupName").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let stream = build.get("logStreamName").and_then(|v| v.as_str());

    writeln!(out)?;
    out.set_color(ColorSpec::new().set_dimmed(true))?;
    match stream {
        Some(stream) => writeln!(out, "Raw container build output: {}/{}", group, stream)?,
        None => writeln!(out, "Raw container build output: {}", group)?,
    }
    out.reset()?;
    Ok(())
}

#[derive(Debug)]
pub(crate) struct LogsCommand;

impl LogsCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }

    /// Resolve the deployment whose logs to print: the one named on the command
    /// line, or the most recent for the environment/region filters.
    fn resolve_deployment(&self, matches: &ArgMatches, app: &str) -> Result<DeploymentSummary> {
        if let Some(id) = matches.get_one::<String>("deployment") {
            let url = format!("{}/deployments/{}", get_platform_management_api_url(), id);
            let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            if response.status().as_u16() == 404 {
                bail!("Deployment '{}' not found.", id);
            }
            if !response.status().is_success() {
                bail!(
                    "Failed to get deployment: {}",
                    response.text().unwrap_or_default()
                );
            }
            return response
                .json()
                .with_context(|| "Failed to parse deployment response");
        }

        // No id given — the platform returns deployments newest-first, so the
        // head of a one-item page is the latest for these filters.
        let mut url = format!(
            "{}/deployments/?applicationId={}&limit=1",
            get_platform_management_api_url(),
            app
        );
        if let Some(environment) = matches.get_one::<String>("environment") {
            url.push_str(&format!("&environment={}", environment));
        }
        if let Some(region) = matches.get_one::<String>("region") {
            url.push_str(&format!("&region={}", region));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list deployments: {}",
                response.text().unwrap_or_default()
            );
        }
        let list: DeploymentListResponse = response
            .json()
            .with_context(|| "Failed to parse deployment list response")?;

        list.deployments
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No deployments found for the given filters."))
    }
}

impl CliCommand for LogsCommand {
    fn command(&self) -> Command {
        command(
            "logs",
            "Show deployment logs (latest for the environment/region, or one by id)",
        )
        .arg(
            Arg::new("deployment")
                .help("Deployment id (defaults to the latest deployment)")
                .index(1),
        )
        .arg(
            Arg::new("environment")
                .long("environment")
                .short('e')
                .help("Filter to an environment when picking the latest deployment"),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .short('r')
                .help("Filter to a region when picking the latest deployment"),
        )
        .arg(
            Arg::new("tail")
                .long("tail")
                .short('n')
                .help("Number of trailing lines to show (default 200)"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("Show the whole log instead of the tail"),
        )
        .arg(
            Arg::new("level")
                .long("level")
                .short('l')
                .help("Only show lines at this level (info, warn, error)"),
        )
        .arg(
            Arg::new("search")
                .long("search")
                .short('s')
                .help("Only show lines whose message contains this text"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let tail = match matches.get_one::<String>("tail") {
            Some(raw) => raw
                .parse::<usize>()
                .with_context(|| format!("--tail expects a number, got '{}'", raw))?,
            None => DEFAULT_TAIL,
        };
        let show_all = matches.get_flag("all");

        let deployment = self.resolve_deployment(matches, &app)?;
        let Some(deployment_id) = deployment.id.clone() else {
            bail!("Deployment response did not include an id.");
        };

        // `limit=0` means "no limit" to the platform. Filtering happens
        // server-side, but the *window* has to be taken here: the API pages
        // from the front, and what is wanted is the end.
        let mut url = format!(
            "{}/deployments/{}/logs?limit=0",
            get_platform_management_api_url(),
            deployment_id
        );
        if let Some(level) = matches.get_one::<String>("level") {
            url.push_str(&format!("&level={}", level));
        }
        if let Some(search) = matches.get_one::<String>("search") {
            url.push_str(&format!("&search={}", urlencoding::encode(search)));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if response.status().as_u16() == 404 {
            bail!("Deployment '{}' not found.", deployment_id);
        }
        if !response.status().is_success() {
            bail!(
                "Failed to get deployment logs: {}",
                response.text().unwrap_or_default()
            );
        }
        let body: DeploymentLogsResponse = response
            .json()
            .with_context(|| "Failed to parse deployment logs response")?;

        let matched = body.logs.len();
        let shown = if show_all { matched } else { tail.min(matched) };
        let skipped = matched - shown;

        // Header: which deployment this is, so a defaulted lookup is never
        // ambiguous about what it picked.
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        write!(&mut stdout, "{}", deployment_id)?;
        stdout.reset()?;
        let mut context: Vec<String> = Vec::new();
        if let Some(environment) = &deployment.environment {
            context.push(environment.clone());
        }
        if let Some(region) = &deployment.region {
            context.push(region.clone());
        }
        if let Some(status) = &deployment.status {
            context.push(status.clone());
        }
        if !context.is_empty() {
            stdout.set_color(ColorSpec::new().set_dimmed(true))?;
            write!(&mut stdout, "  ({})", context.join(" · "))?;
            stdout.reset()?;
        }
        writeln!(&mut stdout)?;

        if matched == 0 {
            stdout.set_color(ColorSpec::new().set_dimmed(true))?;
            if body.total_lines > 0 {
                writeln!(
                    &mut stdout,
                    "No log lines matched (deployment has {} lines).",
                    body.total_lines
                )?;
            } else {
                writeln!(&mut stdout, "No logs recorded for this deployment yet.")?;
            }
            stdout.reset()?;
            print_build_log_location(&mut stdout, &deployment.metadata)?;
            return Ok(());
        }

        if skipped > 0 {
            stdout.set_color(ColorSpec::new().set_dimmed(true))?;
            writeln!(
                &mut stdout,
                "showing last {} of {} lines — use --tail <n> or --all for more",
                shown, matched
            )?;
            stdout.reset()?;
        }
        writeln!(&mut stdout)?;

        for entry in &body.logs[matched - shown..] {
            print_entry(&mut stdout, entry)?;
        }

        print_build_log_location(&mut stdout, &deployment.metadata)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn logs_cmd() -> Command {
        LogsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        logs_cmd().debug_assert();
    }

    #[test]
    fn the_deployment_id_is_optional() {
        // The whole point of the default: `deploy logs` with no arguments has
        // to resolve to the latest deployment rather than erroring.
        let matches = logs_cmd().try_get_matches_from(["logs"]).unwrap();
        assert_eq!(matches.get_one::<String>("deployment"), None);

        let matches = logs_cmd().try_get_matches_from(["logs", "dep-1"]).unwrap();
        assert_eq!(
            matches.get_one::<String>("deployment").map(String::as_str),
            Some("dep-1")
        );
    }

    #[test]
    fn accepts_the_filter_and_window_flags() {
        let matches = logs_cmd()
            .try_get_matches_from([
                "logs", "-e", "staging", "-r", "us-west-2", "-n", "50", "-l", "error", "-s",
                "KMS", "--all",
            ])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("environment").map(String::as_str),
            Some("staging")
        );
        assert_eq!(
            matches.get_one::<String>("region").map(String::as_str),
            Some("us-west-2")
        );
        assert_eq!(
            matches.get_one::<String>("tail").map(String::as_str),
            Some("50")
        );
        assert_eq!(
            matches.get_one::<String>("level").map(String::as_str),
            Some("error")
        );
        assert_eq!(
            matches.get_one::<String>("search").map(String::as_str),
            Some("KMS")
        );
        assert!(matches.get_flag("all"));
    }

    #[test]
    fn parses_the_structured_logs_response() {
        let body = r#"{
            "logs": [
                {"timestamp":"2026-08-31T09:08:53.885Z","level":"error","message":"boom","step":"AppDeployment"},
                {"timestamp":"2026-08-31T09:08:54.000Z","level":"info","message":"ok"}
            ],
            "totalLines": 2,
            "truncated": false
        }"#;
        let parsed: DeploymentLogsResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.total_lines, 2);
        assert_eq!(parsed.logs.len(), 2);
        assert_eq!(parsed.logs[0].message, "boom");
        assert_eq!(parsed.logs[0].step.as_deref(), Some("AppDeployment"));
        assert_eq!(parsed.logs[1].step, None);
    }

    #[test]
    fn tolerates_entries_missing_optional_fields() {
        let parsed: DeploymentLogsResponse =
            serde_json::from_str(r#"{"logs":[{"message":"bare"}]}"#).unwrap();
        assert_eq!(parsed.total_lines, 0);
        assert_eq!(parsed.logs[0].level, None);
        assert_eq!(parsed.logs[0].timestamp, None);
    }

    #[test]
    fn short_time_extracts_the_clock_portion() {
        assert_eq!(
            short_time(&Some("2026-08-31T09:08:53.885Z".to_string())),
            "09:08:53"
        );
    }

    #[test]
    fn short_time_pads_when_the_timestamp_is_missing_or_odd() {
        assert_eq!(short_time(&None).len(), 8);
        assert_eq!(short_time(&Some("not-a-timestamp".to_string())).len(), 8);
    }

    #[test]
    fn errors_and_warnings_are_colored_and_info_is_not() {
        assert_eq!(level_color("error"), Some(Color::Red));
        assert_eq!(level_color("ERROR"), Some(Color::Red));
        assert_eq!(level_color("warn"), Some(Color::Yellow));
        assert_eq!(level_color("info"), None);
    }

    #[test]
    fn build_log_location_is_optional() {
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        assert!(print_build_log_location(&mut stdout, &None).is_ok());
        assert!(print_build_log_location(&mut stdout, &Some(json!({}))).is_ok());
        assert!(print_build_log_location(&mut stdout, &Some(json!({"codeBuildLogs": {}}))).is_ok());
    }

    #[test]
    fn deployment_summary_carries_the_build_log_location() {
        let summary: DeploymentSummary = serde_json::from_str(
            r#"{"id":"d1","metadata":{"codeBuildLogs":{"buildId":"p:1","logGroupName":"/aws/codebuild/p","logStreamName":"1"}}}"#,
        )
        .unwrap();
        let group = summary
            .metadata
            .as_ref()
            .and_then(|m| m.get("codeBuildLogs"))
            .and_then(|b| b.get("logGroupName"))
            .and_then(|v| v.as_str());
        assert_eq!(group, Some("/aws/codebuild/p"));
    }
}
