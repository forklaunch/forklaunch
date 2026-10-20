use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use create::CreateCommand;
use delete::DeleteCommand;
use update::UpdateCommand;
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{command::command, http_client::get},
};

mod create;
mod delete;
mod update;

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct NotifiersCommand {
    create: CreateCommand,
    delete: DeleteCommand,
    update: UpdateCommand,
}

impl NotifiersCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            delete: DeleteCommand::new(),
            update: UpdateCommand::new(),
        }
    }
}

impl CliCommand for NotifiersCommand {
    fn command(&self) -> Command {
        command(
            "notifiers",
            "List, create, update, or delete notifier configs",
        )
            .arg(
                Arg::new("service")
                    .long("service")
                    .help("Filter to a service name")
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
            .subcommand(self.update.command())
            .subcommand(self.delete.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("update", sub_matches)) => self.update.handler(sub_matches),
            Some(("delete", sub_matches)) => self.delete.handler(sub_matches),
            _ => list_notifiers(matches),
        }
    }
}

// ── List handler ──────────────────────────────────────────────────────────────

fn list_notifiers(matches: &ArgMatches) -> Result<()> {
    let service = matches.get_one::<String>("service");
    let json_output = matches.get_flag("json");

    let mut url = format!("{}/notifier-configs", get_observability_api_url());
    if let Some(s) = service {
        url.push_str(&format!("?serviceName={}", urlencoding::encode(s)));
    }

    let response = get(&url).with_context(|| "Failed to reach observability API")?;

    let status = response.status();
    if !status.is_success() {
        bail!(
            "Failed to list notifier configs ({}): {}",
            status,
            response.text().unwrap_or_default()
        );
    }

    let configs: Vec<NotifierConfig> = response
        .json()
        .with_context(|| "Failed to parse notifier configs response")?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&configs)?);
        return Ok(());
    }

    print_configs(&configs)
}

fn print_configs(configs: &[NotifierConfig]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if configs.is_empty() {
        writeln!(stdout, "No notifier configs found.")?;
        return Ok(());
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<36}  {:<20}  {:<30}  {}",
        "ID", "SERVICE", "SLACK WEBHOOK", "EMAIL"
    )?;
    stdout.reset()?;

    for config in configs {
        let slack = config
            .slack_webhook_url
            .as_deref()
            .map(|_| "configured")
            .unwrap_or("-");
        writeln!(
            stdout,
            "  {:<36}  {:<20}  {:<30}  {}",
            config.id,
            config.service_name.as_deref().unwrap_or("-"),
            slack,
            config.email.as_deref().unwrap_or("-")
        )?;
    }
    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotifierConfig {
    id: String,
    #[serde(default)]
    service_name: Option<String>,
    #[serde(default)]
    slack_webhook_url: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notifiers_cmd() -> Command {
        NotifiersCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        notifiers_cmd().debug_assert();
    }

    #[test]
    fn list_has_no_required_args() {
        assert!(notifiers_cmd().try_get_matches_from(["notifiers"]).is_ok());
    }

    #[test]
    fn delete_requires_id() {
        assert!(
            notifiers_cmd()
                .try_get_matches_from(["notifiers", "delete"])
                .is_err()
        );
        assert!(
            notifiers_cmd()
                .try_get_matches_from(["notifiers", "delete", "cfg-1"])
                .is_ok()
        );
    }

    #[test]
    fn create_has_no_required_clap_args() {
        // The at-least-one-of-slack/email check happens at runtime, not via clap,
        // since neither flag is individually required.
        assert!(
            notifiers_cmd()
                .try_get_matches_from(["notifiers", "create"])
                .is_ok()
        );
    }

    #[test]
    fn notifier_config_deserializes() {
        let json = r#"{"id": "cfg-1", "serviceName": "iam", "slackWebhookUrl": "https://hooks.slack.com/x"}"#;
        let config: NotifierConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.id, "cfg-1");
        assert_eq!(config.service_name.as_deref(), Some("iam"));
    }

    #[test]
    fn notifier_configs_list_deserializes_bare_array() {
        let json = r#"[{"id":"cfg-1"}]"#;
        let configs: Vec<NotifierConfig> = serde_json::from_str(json).unwrap();
        assert_eq!(configs.len(), 1);
    }
}
