use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{ArgMatches, Command};
use create::CreateCommand;
use link::LinkCommand;
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};
use unlink::UnlinkCommand;
use validate::ValidateCommand;

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

mod create;
mod link;
mod unlink;
mod validate;

// ── Top-level command ─────────────────────────────────────────────────────────
// BYOC (bring-your-own-cloud) only — most customers use ForkLaunch-managed
// hosting and will never touch this command group.

#[derive(Debug)]
pub(crate) struct CloudAccountCommand {
    create: CreateCommand,
    link: LinkCommand,
    unlink: UnlinkCommand,
    validate: ValidateCommand,
}

impl CloudAccountCommand {
    pub(crate) fn new() -> Self {
        Self {
            create: CreateCommand::new(),
            link: LinkCommand::new(),
            unlink: UnlinkCommand::new(),
            validate: ValidateCommand::new(),
        }
    }
}

impl CliCommand for CloudAccountCommand {
    fn command(&self) -> Command {
        command(
            "cloud-account",
            "Manage a bring-your-own-cloud (BYOC) account link for this organization",
        )
        .subcommand(self.create.command())
        .subcommand(self.link.command())
        .subcommand(self.unlink.command())
        .subcommand(self.validate.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("link", sub_matches)) => self.link.handler(sub_matches),
            Some(("unlink", sub_matches)) => self.unlink.handler(sub_matches),
            Some(("validate", sub_matches)) => self.validate.handler(sub_matches),
            _ => show_status(),
        }
    }
}

// ── Status (default action) ───────────────────────────────────────────────────

fn show_status() -> Result<()> {
    let _token = require_auth()?;

    let url = format!("{}/cloud-accounts", get_platform_management_api_url());
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

    if response.status().as_u16() == 404 || response.status().as_u16() == 204 {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(
            stdout,
            "No cloud account linked. Run `fl cloud-account create` to start."
        )?;
        return Ok(());
    }
    if !response.status().is_success() {
        bail!(
            "Failed to get cloud account: {}",
            response.text().unwrap_or_default()
        );
    }

    let account: CloudAccount = response
        .json()
        .with_context(|| "Failed to parse cloud account response")?;

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    writeln!(stdout)?;
    writeln!(stdout, "  provider:  {}", account.provider)?;
    writeln!(stdout, "  status:    {}", account.status)?;
    if let Some(arn) = &account.role_arn {
        writeln!(stdout, "  role arn:  {}", arn)?;
    }
    if let Some(err) = &account.validation_error {
        writeln!(stdout, "  error:     {}", err)?;
    }
    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudAccount {
    #[serde(default)]
    id: Option<String>,
    provider: String,
    status: String,
    #[serde(default)]
    role_arn: Option<String>,
    #[serde(default)]
    validation_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud_account_cmd() -> Command {
        CloudAccountCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cloud_account_cmd().debug_assert();
    }

    #[test]
    fn status_has_no_required_args() {
        assert!(cloud_account_cmd().try_get_matches_from(["cloud-account"]).is_ok());
    }
}
