use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::NotifierConfig;
use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{command::command, http_client::post},
};

#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Create a notifier config")
            .arg(
                Arg::new("slack_webhook_url")
                    .long("slack-webhook")
                    .help("Slack incoming webhook URL to notify"),
            )
            .arg(
                Arg::new("email")
                    .long("email")
                    .help("Email address to notify"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let service = matches.get_one::<String>("service");
        let slack = matches.get_one::<String>("slack_webhook_url");
        let email = matches.get_one::<String>("email");
        let json_output = matches.get_flag("json");

        if slack.is_none() && email.is_none() {
            bail!("Provide at least one of --slack-webhook or --email");
        }

        let mut body = serde_json::json!({});
        if let Some(s) = service {
            body["serviceName"] = serde_json::Value::String(s.clone());
        }
        if let Some(s) = slack {
            body["slackWebhookUrl"] = serde_json::Value::String(s.clone());
        }
        if let Some(e) = email {
            body["email"] = serde_json::Value::String(e.clone());
        }

        let url = format!("{}/notifier-configs", get_observability_api_url());
        let response = post(&url, body).with_context(|| "Failed to reach observability API")?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to create notifier config ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let config: NotifierConfig = response
            .json()
            .with_context(|| "Failed to parse notifier config response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&config)?);
        } else {
            let mut stdout = StandardStream::stdout(ColorChoice::Always);
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
            write!(stdout, "  Created")?;
            stdout.reset()?;
            writeln!(stdout, "  notifier config {}", config.id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_cmd() -> Command {
        CreateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        create_cmd().debug_assert();
    }

    #[test]
    fn has_no_required_clap_args() {
        // The at-least-one-of-slack/email check happens at runtime, not via clap,
        // since neither flag is individually required.
        assert!(create_cmd().try_get_matches_from(["create"]).is_ok());
    }
}
