use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use create::CreateCommand;
use delete::DeleteCommand;
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::get,
        validate::{require_integration, require_manifest},
    },
};

mod create;
mod delete;

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct AlertsCommand {
    create: CreateCommand,
    delete: DeleteCommand,
}

impl AlertsCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            delete: DeleteCommand::new(),
        }
    }
}

impl CliCommand for AlertsCommand {
    fn command(&self) -> Command {
        command("alerts", "List, create, or delete alert rules")
            .subcommand_negates_reqs(true)
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path")
                    .global(true),
            )
            .arg(
                Arg::new("env")
                    .short('e')
                    .long("env")
                    .help("Environment to inspect (for example: dev, staging, production) — required for `list` and `create`")
                    .global(true),
            )
            .arg(
                Arg::new("service")
                    .long("service")
                    .help("Filter to a service id")
                    .global(true),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue)
                    .global(true),
            )
            .subcommand(self.create.command())
            .subcommand(self.delete.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("delete", sub_matches)) => self.delete.handler(sub_matches),
            _ => list_alerts(matches),
        }
    }
}

// ── List handler ──────────────────────────────────────────────────────────────

fn list_alerts(matches: &ArgMatches) -> Result<()> {
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let env = matches
        .get_one::<String>("env")
        .context("--env is required for listing alert rules")?;
    let service = matches.get_one::<String>("service");
    let json_output = matches.get_flag("json");

    let mut url = format!(
        "{}/alert-rules?appId={}&env={}",
        get_observability_api_url(),
        urlencoding::encode(&application_id),
        urlencoding::encode(env),
    );
    if let Some(s) = service {
        url.push_str(&format!("&serviceId={}", urlencoding::encode(s)));
    }

    let response = get(&url).with_context(|| "Failed to reach observability API")?;

    let status = response.status();
    if !status.is_success() {
        bail!(
            "Failed to list alert rules ({}): {}",
            status,
            response.text().unwrap_or_default()
        );
    }

    let rules: Vec<AlertRule> = response
        .json()
        .with_context(|| "Failed to parse alert rules response")?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&rules)?);
        return Ok(());
    }

    print_rules(&rules)
}

fn print_rules(rules: &[AlertRule]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if rules.is_empty() {
        writeln!(stdout, "No alert rules found.")?;
        return Ok(());
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<36}  {:<20}  {:<6}  {:<10}  {:<8}  {}",
        "ID", "METRIC", "OP", "VALUE", "WINDOW", "SEVERITY"
    )?;
    stdout.reset()?;

    for rule in rules {
        writeln!(
            stdout,
            "  {:<36}  {:<20}  {:<6}  {:<10}  {:<8}  {}",
            rule.id, rule.metric_name, rule.operator, rule.value, rule.window_size, rule.severity
        )?;
    }
    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlertRule {
    id: String,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    service_id: Option<String>,
    #[serde(default)]
    env: Option<String>,
    metric_name: String,
    operator: String,
    value: f64,
    window_size: String,
    severity: String,
    #[serde(default)]
    notifier_config_id: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alerts_cmd() -> Command {
        AlertsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        alerts_cmd().debug_assert();
    }

    #[test]
    fn env_parses_when_given() {
        // --env can't be both `global` and `required` in clap (global args
        // can't be required), so list_alerts() enforces it at runtime instead
        // — see the `.context("--env is required...")` call in the handler.
        assert!(
            alerts_cmd()
                .try_get_matches_from(["alerts", "-e", "dev"])
                .is_ok()
        );
    }

    #[test]
    fn create_does_not_require_env_at_parse_time_but_needs_its_own_flags() {
        // env is negated for subcommands by subcommand_negates_reqs, but create's
        // own required flags still apply.
        assert!(
            alerts_cmd()
                .try_get_matches_from(["alerts", "create"])
                .is_err()
        );
        assert!(
            alerts_cmd()
                .try_get_matches_from([
                    "alerts",
                    "create",
                    "--metric",
                    "error_rate",
                    "--operator",
                    "GT",
                    "--value",
                    "0.05",
                    "--window",
                    "5m",
                    "--severity",
                    "ALERT",
                ])
                .is_ok()
        );
    }

    #[test]
    fn delete_requires_id() {
        assert!(
            alerts_cmd()
                .try_get_matches_from(["alerts", "delete"])
                .is_err()
        );
        assert!(
            alerts_cmd()
                .try_get_matches_from(["alerts", "delete", "rule-1"])
                .is_ok()
        );
    }

    #[test]
    fn alert_rule_deserializes() {
        let json = r#"{
            "id": "rule-1",
            "appId": "app-1",
            "env": "production",
            "metricName": "error_rate",
            "operator": "GT",
            "value": 0.05,
            "windowSize": "5m",
            "severity": "ALERT",
            "enabled": true
        }"#;
        let rule: AlertRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "rule-1");
        assert_eq!(rule.operator, "GT");
    }

    #[test]
    fn alert_rules_list_deserializes_bare_array() {
        let json = r#"[{"id":"r1","metricName":"m","operator":"GT","value":1.0,"windowSize":"5m","severity":"ALERT"}]"#;
        let rules: Vec<AlertRule> = serde_json::from_str(json).unwrap();
        assert_eq!(rules.len(), 1);
    }
}
