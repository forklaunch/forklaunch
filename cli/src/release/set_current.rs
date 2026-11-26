use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client::{self, make_authenticated_request},
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// `release set-current <version>` — marks which release an application's
/// dashboards, variable defaults and readiness checks read from. The route
/// existed and no surface called it.
#[derive(Debug)]
pub(crate) struct SetCurrentCommand;

impl SetCurrentCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseRow {
    id: String,
    version: String,
}

/// `GET /releases` answers `{ releases: [...] }`, not a bare array.
#[derive(Debug, Deserialize)]
struct ReleaseListResponse {
    #[serde(default)]
    releases: Vec<ReleaseRow>,
}

impl CliCommand for SetCurrentCommand {
    fn command(&self) -> Command {
        command(
            "set-current",
            "Mark a release as this application's current release",
        )
        // The positional is `version`, which collides with clap's auto --version.
        .disable_version_flag(true)
        .long_about(
            "Mark a release as this application's current release.\n\n\
             This changes which release the dashboard, environment-variable defaults \
             and readiness checks read from. It does NOT deploy: use \
             `forklaunch deploy create -r <version>` for that.",
        )
        .arg(
            Arg::new("version")
                .required(true)
                .help("Release version to make current (e.g. 1.2.3)"),
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
        let version = matches
            .get_one::<String>("version")
            .context("version is required")?;

        // The route takes a release id; people know versions.
        let list_url = format!(
            "{}/releases?applicationId={}",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id)
        );
        let response = http_client::get(&list_url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list releases: {}",
                response.text().unwrap_or_default()
            );
        }
        let releases = response
            .json::<ReleaseListResponse>()
            .with_context(|| "Failed to parse releases response")?
            .releases;
        let release = releases
            .iter()
            .find(|r| r.version == *version)
            .ok_or_else(|| {
                let known: Vec<&str> = releases
                    .iter()
                    .map(|r| r.version.as_str())
                    .take(10)
                    .collect();
                anyhow::anyhow!(
                    "No release {} for this application. Known versions: {}",
                    version,
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                )
            })?;

        let url = format!(
            "{}/releases/{}/set-current",
            get_platform_management_api_url(),
            urlencoding::encode(&release.id)
        );
        let response = make_authenticated_request(
            Method::PUT,
            &url,
            Some(serde_json::json!({ "applicationId": application_id })),
        )
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to set current release ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Current")?;
        stdout.reset()?;
        writeln!(stdout, "  release is now {}", version)?;
        writeln!(
            stdout,
            "  This did not deploy anything: forklaunch deploy create -r {} -e <env> --region <region>",
            version
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        SetCurrentCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn requires_a_version() {
        assert!(cmd().try_get_matches_from(["set-current"]).is_err());
        assert!(cmd().try_get_matches_from(["set-current", "1.2.3"]).is_ok());
    }
}
