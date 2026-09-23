use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_manifest},
    },
    github::{application_github_url, github_app_url},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppStatusResponse {
    installed: bool,
    installation_id: Option<String>,
    account_login: Option<String>,
    account_type: Option<String>,
}

#[derive(Debug)]
pub(crate) struct StatusCommand;

impl StatusCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for StatusCommand {
    fn command(&self) -> Command {
        command(
            "status",
            "Show GitHub App installation status and this app's repository connection",
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        require_auth()?;

        let url = github_app_url("status");
        let response = http_client::get(&url)?;
        if !response.status().is_success() {
            bail!(
                "Failed to fetch GitHub App status (Status: {})",
                response.status()
            );
        }
        let status: AppStatusResponse = response
            .json()
            .with_context(|| "Failed to parse GitHub App status response")?;

        if status.installed {
            log_ok!(
                stdout,
                "GitHub App installed for {} ({})",
                status.account_login.as_deref().unwrap_or("unknown"),
                status.account_type.as_deref().unwrap_or("unknown")
            );
            if let Some(id) = &status.installation_id {
                log_info!(stdout, "Installation ID: {}", id);
            }
        } else {
            log_warn!(
                stdout,
                "GitHub App is not installed for this organization. Run `forklaunch github install` to get the installation link."
            );
        }

        // Per-application connection settings when run inside an integrated app
        let Ok((_, manifest)) = require_manifest(matches) else {
            return Ok(());
        };
        let Some(app_id) = &manifest.platform_application_id else {
            return Ok(());
        };
        let settings_url = application_github_url(app_id, "settings");
        // Silence here used to be indistinguishable from "no repository
        // connected": the request 404'd on a wrong URL and both arms below
        // returned Ok(()) without a word. Say what happened instead.
        let resp = match http_client::get(&settings_url) {
            Ok(resp) => resp,
            Err(error) => {
                log_warn!(
                    stdout,
                    "Could not read this application's repository settings: {}",
                    error
                );
                return Ok(());
            }
        };
        if !resp.status().is_success() {
            log_warn!(
                stdout,
                "Could not read this application's repository settings (status {}).",
                resp.status()
            );
            return Ok(());
        }
        let body: serde_json::Value = resp
            .json()
            .with_context(|| "Failed to parse application settings")?;
        log_header!(stdout, Color::Cyan, "\nApplication repository settings:");
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;

        Ok(())
    }
}
