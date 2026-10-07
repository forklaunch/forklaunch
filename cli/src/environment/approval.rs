use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client::make_authenticated_request,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// `environment approval` — whether deploys to an environment park behind the
/// approval gate. Tri-state on the server: true/false override the default,
/// and clearing it falls back to "required for production-named environments".
#[derive(Debug)]
pub(crate) struct ApprovalCommand {
    require: RequireCommand,
    waive: WaiveCommand,
    reset: ResetCommand,
}

impl ApprovalCommand {
    pub(crate) fn new() -> Self {
        Self {
            require: RequireCommand::new(),
            waive: WaiveCommand::new(),
            reset: ResetCommand::new(),
        }
    }
}

impl CliCommand for ApprovalCommand {
    fn command(&self) -> Command {
        command(
            "approval",
            "Require, waive, or reset the deployment-approval gate for an environment",
        )
        .subcommand_required(true)
        .subcommand(self.require.command())
        .subcommand(self.waive.command())
        .subcommand(self.reset.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("require", sub_matches)) => self.require.handler(sub_matches),
            Some(("waive", sub_matches)) => self.waive.handler(sub_matches),
            Some(("reset", sub_matches)) => self.reset.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

fn args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("environment")
            .long("environment")
            .short('e')
            .required(true)
            .help("Environment name"),
    )
    .arg(
        Arg::new("base_path")
            .short('p')
            .long("path")
            .help("Path to application root (optional)"),
    )
}

fn patch_approval(matches: &ArgMatches, value: Option<bool>) -> Result<()> {
    let _token = require_auth()?;
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let environment = matches
        .get_one::<String>("environment")
        .context("environment is required")?;

    let url = format!(
        "{}/applications/{}/environments/{}/approval-config",
        get_platform_management_api_url(),
        urlencoding::encode(&application_id),
        urlencoding::encode(environment)
    );
    let body = match value {
        Some(v) => serde_json::json!({ "requireDeploymentApproval": v }),
        None => serde_json::json!({ "requireDeploymentApproval": serde_json::Value::Null }),
    };
    let response = make_authenticated_request(Method::PATCH, &url, Some(body))
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

    let status = response.status();
    if !status.is_success() {
        bail!(
            "Failed to update approval config ({}): {}",
            status,
            response.text().unwrap_or_default()
        );
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
    write!(stdout, "  Updated")?;
    stdout.reset()?;
    match value {
        Some(true) => writeln!(stdout, "  deploys to {} now require approval", environment)?,
        Some(false) => writeln!(
            stdout,
            "  deploys to {} no longer require approval",
            environment
        )?,
        None => writeln!(
            stdout,
            "  {} is back to the default (approval required for production-named environments)",
            environment
        )?,
    }
    writeln!(
        stdout,
        "  Pending approvals: forklaunch deploy approvals list"
    )?;
    Ok(())
}

#[derive(Debug)]
struct RequireCommand;
impl RequireCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for RequireCommand {
    fn command(&self) -> Command {
        args(command(
            "require",
            "Require an admin approval before deploys to this environment run",
        ))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        patch_approval(matches, Some(true))
    }
}

#[derive(Debug)]
struct WaiveCommand;
impl WaiveCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for WaiveCommand {
    fn command(&self) -> Command {
        args(command(
            "waive",
            "Let deploys to this environment run without an approval",
        ))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        patch_approval(matches, Some(false))
    }
}

#[derive(Debug)]
struct ResetCommand;
impl ResetCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ResetCommand {
    fn command(&self) -> Command {
        args(command(
            "reset",
            "Fall back to the default (required for production-named environments)",
        ))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        patch_approval(matches, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        ApprovalCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }

    #[test]
    fn every_subcommand_requires_an_environment() {
        let c = ApprovalCommand::new().command().version("0.0.0-test");
        for sub in ["require", "waive", "reset"] {
            assert!(
                c.clone().try_get_matches_from(["approval", sub]).is_err(),
                "{sub} must require -e"
            );
            assert!(
                c.clone()
                    .try_get_matches_from(["approval", sub, "-e", "production"])
                    .is_ok()
            );
        }
    }
}
