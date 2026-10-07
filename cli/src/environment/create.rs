use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
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

/// `environment create` — the dashboard could add an environment to an
/// application; a script could only deploy into one that already existed, and
/// `deploy create` would offer to create it as a side effect of deploying.
#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Add an environment to this application")
            .arg(
                Arg::new("environment")
                    .long("environment")
                    .short('e')
                    .required(true)
                    .help("Environment name (e.g. staging, production)"),
            )
            .arg(
                Arg::new("region")
                    .long("region")
                    .required(true)
                    .help("AWS region the environment runs in (e.g. us-west-1)"),
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
            .context("environment is required")?
            .to_lowercase();
        let region = matches
            .get_one::<String>("region")
            .context("region is required")?;

        let url = format!(
            "{}/applications/{}/environments",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id)
        );
        let response = http_client::post(
            &url,
            serde_json::json!({ "name": environment, "region": region }),
        )
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to create environment ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Created")?;
        stdout.reset()?;
        writeln!(stdout, "  environment {} in {}", environment, region)?;
        writeln!(
            stdout,
            "  Deploy to it with: forklaunch deploy create -r <version> -e {} --region {}",
            environment, region
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        CreateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn environment_and_region_are_both_required() {
        assert!(cmd().try_get_matches_from(["create"]).is_err());
        assert!(
            cmd()
                .try_get_matches_from(["create", "-e", "staging"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["create", "-e", "staging", "--region", "us-west-1"])
                .is_ok()
        );
    }
}
