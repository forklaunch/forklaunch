use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{command::command, http_client::put},
};

/// `notifiers update` — the route existed and NO surface called it: the CLI
/// could create and delete a notifier config, the dashboard could list them,
/// and nothing could edit one. Rotating a Slack webhook meant delete + create.
#[derive(Debug)]
pub(crate) struct UpdateCommand;

impl UpdateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for UpdateCommand {
    fn command(&self) -> Command {
        command("update", "Update an existing notifier config")
            .arg(
                Arg::new("id")
                    .required(true)
                    .help("The notifier config ID to update"),
            )
            .arg(
                Arg::new("slack_webhook_url")
                    .long("slack-webhook")
                    .help("New Slack incoming webhook URL"),
            )
            .arg(
                Arg::new("email")
                    .long("email")
                    .help("New email address to notify"),
            )
            .arg(
                Arg::new("service_name")
                    .long("service-name")
                    .help("Re-scope this config to a different service name"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("notifier config id is required")?;
        let slack = matches.get_one::<String>("slack_webhook_url");
        let email = matches.get_one::<String>("email");
        let service_name = matches.get_one::<String>("service_name");

        if slack.is_none() && email.is_none() && service_name.is_none() {
            bail!(
                "Nothing to update. Pass at least one of --slack-webhook, --email or --service-name."
            );
        }

        let mut body = serde_json::json!({});
        if let Some(s) = service_name {
            body["serviceName"] = serde_json::Value::String(s.clone());
        }
        if let Some(s) = slack {
            body["slackWebhookUrl"] = serde_json::Value::String(s.clone());
        }
        if let Some(e) = email {
            body["email"] = serde_json::Value::String(e.clone());
        }

        let url = format!(
            "{}/notifier-configs/{}",
            get_observability_api_url(),
            urlencoding::encode(id)
        );
        let response = put(&url, body).with_context(|| "Failed to reach observability API")?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to update notifier config ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Updated")?;
        stdout.reset()?;
        writeln!(stdout, "  notifier config {}", id)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        UpdateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn requires_an_id() {
        assert!(cmd().try_get_matches_from(["update"]).is_err());
        assert!(
            cmd()
                .try_get_matches_from(["update", "cfg-1", "--email", "a@b.c"])
                .is_ok()
        );
    }
}
