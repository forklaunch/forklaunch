use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Confirm, theme::ColorfulTheme};
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

/// `environment delete` — destructive, so it names the environment back and
/// asks, the way `deploy destroy` does. A non-interactive run must pass --yes.
#[derive(Debug)]
pub(crate) struct DeleteCommand;

impl DeleteCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for DeleteCommand {
    fn command(&self) -> Command {
        command("delete", "Remove an environment from this application")
            .long_about(
                "Remove an environment from this application.\n\n\
                 This deletes the environment record and its stored configuration. \
                 Deployed infrastructure is NOT torn down by this command — run \
                 `forklaunch deploy destroy` first if the environment still has \
                 running services.",
            )
            .arg(
                Arg::new("environment")
                    .long("environment")
                    .short('e')
                    .required(true)
                    .help("Environment name to delete"),
            )
            .arg(
                Arg::new("yes")
                    .long("yes")
                    .short('y')
                    .action(ArgAction::SetTrue)
                    .help("Skip the confirmation prompt"),
            )
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("Path to application root (optional)"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let environment = matches
            .get_one::<String>("environment")
            .context("environment is required")?;
        let assume_yes = matches.get_flag("yes");

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if !assume_yes {
            if !std::io::stdin().is_terminal() {
                bail!(
                    "Refusing to delete environment \"{}\" without a terminal. Pass --yes to confirm.",
                    environment
                );
            }
            writeln!(
                stdout,
                "About to delete environment \"{}\" from {}.",
                environment, manifest.app_name
            )?;
            writeln!(
                stdout,
                "Deployed infrastructure is not torn down by this; run `deploy destroy` for that."
            )?;
            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Delete it?")
                .default(false)
                .interact()?;
            if !confirmed {
                writeln!(stdout, "Cancelled.")?;
                return Ok(());
            }
        }

        let url = format!(
            "{}/applications/{}/environments/{}",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id),
            urlencoding::encode(environment)
        );
        let response = make_authenticated_request(Method::DELETE, &url, None)
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to delete environment ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Deleted")?;
        stdout.reset()?;
        writeln!(stdout, "  environment {}", environment)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        DeleteCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn environment_is_required_and_yes_is_a_flag() {
        assert!(cmd().try_get_matches_from(["delete"]).is_err());
        let m = cmd()
            .try_get_matches_from(["delete", "-e", "staging", "--yes"])
            .unwrap();
        assert!(m.get_flag("yes"));
    }
}
