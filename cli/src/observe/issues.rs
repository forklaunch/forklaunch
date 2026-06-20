use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        hmac::AuthMode,
        http_client::{get_with_auth, post_with_auth},
        validate::{require_integration, require_manifest},
    },
};

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct IssuesCommand {
    ack: AckCommand,
}

impl IssuesCommand {
    pub(super) fn new() -> Self {
        Self {
            ack: AckCommand::new(),
        }
    }
}

impl CliCommand for IssuesCommand {
    fn command(&self) -> Command {
        command("issues", "List or acknowledge active issues for a ForkLaunch application")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("environment")
                    .short('e')
                    .long("environment")
                    .help("Environment to inspect (for example: dev, staging, production)"),
            )
            .arg(
                Arg::new("severity")
                    .long("severity")
                    .help("Filter by severity (ERROR, ALERT, INCIDENT)"),
            )
            .arg(
                Arg::new("status")
                    .long("status")
                    .help("Filter by status (for example: open, acknowledged)"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
            .subcommand(self.ack.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("ack", sub_matches)) => self.ack.handler(sub_matches),
            _ => list_issues(matches),
        }
    }
}

// ── Ack sub-subcommand ────────────────────────────────────────────────────────

#[derive(Debug)]
struct AckCommand;

impl AckCommand {
    fn new() -> Self {
        Self
    }
}

impl CliCommand for AckCommand {
    fn command(&self) -> Command {
        command("ack", "Acknowledge an active issue")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("id")
                    .required(true)
                    .help("The issue ID to acknowledge"),
            )
            .arg(
                Arg::new("acknowledged_by")
                    .long("acknowledged-by")
                    .help("User acknowledging the issue (defaults to the logged-in user's email)"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        acknowledge_issue(matches)
    }
}

// ── List handler ──────────────────────────────────────────────────────────────

fn list_issues(matches: &ArgMatches) -> Result<()> {
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let environment = matches
        .get_one::<String>("environment")
        .context("--environment is required for listing issues")?
        .to_string();
    let severity = matches.get_one::<String>("severity").cloned();
    let status = matches.get_one::<String>("status").cloned();
    let json_output = matches.get_flag("json");

    let response = fetch_issues(&application_id, &environment, severity.as_deref(), status.as_deref())?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_issues(&response.issues)?;
    }

    Ok(())
}

fn fetch_issues(
    application_id: &str,
    environment: &str,
    severity: Option<&str>,
    status: Option<&str>,
) -> Result<IssuesResponse> {
    let api_url = get_observability_api_url();
    let mut url = format!(
        "{}/issues?appId={}&env={}",
        api_url,
        urlencoding::encode(application_id),
        urlencoding::encode(environment),
    );
    if let Some(sev) = severity {
        url.push_str(&format!("&severity={}", urlencoding::encode(sev)));
    }
    if let Some(st) = status {
        url.push_str(&format!("&status={}", urlencoding::encode(st)));
    }

    let auth_mode = AuthMode::detect();
    let response =
        get_with_auth(&auth_mode, &url).with_context(|| "Failed to reach observability API")?;

    if !response.status().is_success() {
        let http_status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Observability API returned {} — {}", http_status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse issues response")
}

// ── Acknowledge handler ───────────────────────────────────────────────────────

fn acknowledge_issue(matches: &ArgMatches) -> Result<()> {
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let issue_id = matches
        .get_one::<String>("id")
        .context("issue id is required")?
        .to_string();
    let acknowledged_by = matches
        .get_one::<String>("acknowledged_by")
        .cloned()
        .unwrap_or_else(|| resolve_current_user(&application_id));
    let json_output = matches.get_flag("json");

    let response = post_acknowledge(&issue_id, &acknowledged_by)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_ack_result(&issue_id, &acknowledged_by, &response)?;
    }

    Ok(())
}

/// Best-effort attempt to identify the current user for the default
/// `--acknowledged-by` value. Falls back gracefully to "unknown" so that the
/// acknowledge POST can still proceed without forcing the user to supply the
/// flag.
fn resolve_current_user(_application_id: &str) -> String {
    // Try the FORKLAUNCH_USER env var first (useful in CI / scripted contexts).
    if let Ok(user) = std::env::var("FORKLAUNCH_USER") {
        if !user.trim().is_empty() {
            return user.trim().to_string();
        }
    }
    // Fall back to the OS user name.
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn post_acknowledge(issue_id: &str, acknowledged_by: &str) -> Result<AckResponse> {
    let api_url = get_observability_api_url();
    let url = format!("{}/issues/{}/acknowledge", api_url, urlencoding::encode(issue_id));

    let body = serde_json::json!({ "acknowledgedBy": acknowledged_by });
    let auth_mode = AuthMode::detect();
    let response =
        post_with_auth(&auth_mode, &url, body).with_context(|| "Failed to reach observability API")?;

    if !response.status().is_success() {
        let http_status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Observability API returned {} — {}", http_status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse acknowledge response")
}

// ── Display ───────────────────────────────────────────────────────────────────

fn print_issues(issues: &[Issue]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if issues.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        writeln!(stdout, "No active issues found.")?;
        stdout.reset()?;
        return Ok(());
    }

    writeln!(stdout)?;

    // Header
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<10}  {:<10}  {:<20}  {:<30}  {}",
        "SEVERITY", "ID", "SERVICE", "TITLE", "FIRST SEEN"
    )?;
    stdout.reset()?;

    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(
        stdout,
        "  {:-<10}  {:-<10}  {:-<20}  {:-<30}  {:-<19}",
        "", "", "", "", ""
    )?;
    stdout.reset()?;

    for issue in issues {
        let severity = issue.severity.as_deref().unwrap_or("UNKNOWN");
        let color = severity_color(severity);

        stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stdout, "  {:<10}", severity)?;
        stdout.reset()?;

        let id_display = issue.id.get(..10).unwrap_or(&issue.id);
        let service = issue.service_name.as_deref().unwrap_or("-");
        let title = issue.title.as_deref().unwrap_or("-");
        let title_truncated = if title.len() > 30 {
            format!("{}…", &title[..29])
        } else {
            title.to_string()
        };
        let first_seen = issue
            .first_seen
            .as_deref()
            .and_then(|ts| ts.get(..19))
            .map(|ts| ts.replace('T', " "))
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            stdout,
            "  {:<10}  {:<20}  {:<30}  {}",
            id_display, service, title_truncated, first_seen
        )?;
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(stdout, "  {} issue(s) found.", issues.len())?;
    stdout.reset()?;
    writeln!(stdout)?;

    Ok(())
}

fn print_ack_result(issue_id: &str, acknowledged_by: &str, _response: &AckResponse) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
    write!(stdout, "  Acknowledged")?;
    stdout.reset()?;
    writeln!(stdout, "  issue {} by {}", issue_id, acknowledged_by)?;
    writeln!(stdout)?;

    Ok(())
}

fn severity_color(severity: &str) -> Color {
    match severity.to_uppercase().as_str() {
        "ERROR" => Color::Red,
        "ALERT" => Color::Yellow,
        "INCIDENT" => Color::Cyan,
        _ => Color::White,
    }
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Issue {
    id: String,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    service_name: Option<String>,
    #[serde(default)]
    env: Option<String>,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    score: Option<f64>,
    #[serde(default)]
    first_seen: Option<String>,
    #[serde(default)]
    last_seen: Option<String>,
    #[serde(default)]
    acknowledged_at: Option<String>,
    #[serde(default)]
    acknowledged_by: Option<String>,
    #[serde(default)]
    resolved_at: Option<String>,
    #[serde(default)]
    resolved_by: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IssuesResponse {
    issues: Vec<Issue>,
    #[serde(default)]
    total: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AckResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    acknowledged_at: Option<String>,
    #[serde(default)]
    acknowledged_by: Option<String>,
    #[serde(flatten)]
    extra: serde_json::Value,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_color_error_is_red() {
        assert_eq!(severity_color("ERROR"), Color::Red);
    }

    #[test]
    fn severity_color_alert_is_yellow() {
        assert_eq!(severity_color("ALERT"), Color::Yellow);
    }

    #[test]
    fn severity_color_incident_is_cyan() {
        assert_eq!(severity_color("INCIDENT"), Color::Cyan);
    }

    #[test]
    fn severity_color_unknown_is_white() {
        assert_eq!(severity_color("CRITICAL"), Color::White);
    }

    #[test]
    fn severity_color_case_insensitive() {
        assert_eq!(severity_color("error"), Color::Red);
        assert_eq!(severity_color("alert"), Color::Yellow);
        assert_eq!(severity_color("incident"), Color::Cyan);
    }

    #[test]
    fn resolve_current_user_env_var_takes_priority() {
        // SAFETY: test-only; single-threaded test runner for this module.
        unsafe { std::env::set_var("FORKLAUNCH_USER", "ci-bot") };
        let user = resolve_current_user("app-123");
        unsafe { std::env::remove_var("FORKLAUNCH_USER") };
        assert_eq!(user, "ci-bot");
    }

    #[test]
    fn resolve_current_user_trims_whitespace() {
        // SAFETY: test-only; single-threaded test runner for this module.
        unsafe { std::env::set_var("FORKLAUNCH_USER", "  alice  ") };
        let user = resolve_current_user("app-123");
        unsafe { std::env::remove_var("FORKLAUNCH_USER") };
        assert_eq!(user, "alice");
    }

    #[test]
    fn resolve_current_user_skips_empty_env_var() {
        // SAFETY: test-only; single-threaded test runner for this module.
        unsafe { std::env::set_var("FORKLAUNCH_USER", "") };
        // Don't assert exact value — just verify it doesn't panic and returns non-empty.
        let user = resolve_current_user("app-123");
        unsafe { std::env::remove_var("FORKLAUNCH_USER") };
        assert!(!user.is_empty());
    }

    #[test]
    fn issue_deserializes_all_optional_fields() {
        let json = r#"{
            "id": "iss-001",
            "appId": "app-123",
            "serviceName": "payment-service",
            "env": "production",
            "severity": "ERROR",
            "status": "open",
            "title": "High error rate",
            "summary": "Error rate exceeded threshold",
            "score": 0.95,
            "firstSeen": "2024-01-15T10:00:00Z",
            "lastSeen": "2024-01-15T11:00:00Z",
            "acknowledgedAt": null,
            "acknowledgedBy": null,
            "resolvedAt": null,
            "resolvedBy": null
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "iss-001");
        assert_eq!(issue.service_name.as_deref(), Some("payment-service"));
        assert_eq!(issue.severity.as_deref(), Some("ERROR"));
        assert_eq!(issue.title.as_deref(), Some("High error rate"));
        assert_eq!(issue.score, Some(0.95));
    }

    #[test]
    fn issue_deserializes_with_only_required_id() {
        let json = r#"{"id": "iss-002"}"#;
        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "iss-002");
        assert!(issue.severity.is_none());
        assert!(issue.service_name.is_none());
    }

    #[test]
    fn issues_response_deserializes() {
        let json = r#"{
            "issues": [
                {"id": "iss-001", "severity": "ALERT"},
                {"id": "iss-002", "severity": "INCIDENT"}
            ],
            "total": 2
        }"#;

        let response: IssuesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.issues.len(), 2);
        assert_eq!(response.total, Some(2));
    }

    #[test]
    fn ack_response_deserializes_partial() {
        let json = r#"{"id": "iss-001", "acknowledgedAt": "2024-01-15T10:05:00Z", "acknowledgedBy": "alice"}"#;
        let response: AckResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id.as_deref(), Some("iss-001"));
        assert_eq!(response.acknowledged_by.as_deref(), Some("alice"));
    }

    #[test]
    fn title_truncation_logic() {
        let long_title = "A".repeat(35);
        let truncated = if long_title.len() > 30 {
            format!("{}…", &long_title[..29])
        } else {
            long_title.clone()
        };
        assert_eq!(truncated.chars().count(), 30); // 29 chars + ellipsis char
    }
}
