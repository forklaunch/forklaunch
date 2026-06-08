use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseSummary {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    git_commit: Option<String>,
    #[serde(default)]
    git_branch: Option<String>,
    #[serde(default)]
    released_by: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReleaseListResponse {
    #[serde(default)]
    releases: Vec<ReleaseSummary>,
}

fn print_field(out: &mut StandardStream, label: &str, value: &Option<String>) -> Result<()> {
    if let Some(v) = value {
        out.set_color(ColorSpec::new().set_bold(true))?;
        write!(out, "  {:<12}", label)?;
        out.reset()?;
        writeln!(out, "{}", v)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct InfoCommand;

impl InfoCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for InfoCommand {
    fn command(&self) -> Command {
        command("info", "Show details for a release (or the most recent releases)")
            .arg(
                Arg::new("version")
                    .long("version")
                    .short('v')
                    .help("Release version to show (omit to list the 5 most recent)"),
            )
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root (optional)"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let url = format!(
            "{}/releases/?applicationId={}&limit=50",
            get_platform_management_api_url(),
            app
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list releases: {}",
                response.text().unwrap_or_default()
            );
        }
        let list: ReleaseListResponse = response
            .json()
            .with_context(|| "Failed to parse release list response")?;

        match matches.get_one::<String>("version") {
            Some(version) => {
                let Some(release) = list
                    .releases
                    .iter()
                    .find(|r| r.version.as_deref() == Some(version))
                else {
                    bail!(
                        "Release '{}' not found. Known versions: {}",
                        version,
                        list.releases
                            .iter()
                            .filter_map(|r| r.version.clone())
                            .take(10)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                };
                writeln!(stdout)?;
                print_field(&mut stdout, "Version:", &release.version)?;
                print_field(&mut stdout, "Status:", &release.status)?;
                print_field(&mut stdout, "Created:", &release.created_at)?;
                print_field(&mut stdout, "Commit:", &release.git_commit)?;
                print_field(&mut stdout, "Branch:", &release.git_branch)?;
                print_field(&mut stdout, "By:", &release.released_by)?;
                print_field(&mut stdout, "Notes:", &release.notes)?;
                print_field(&mut stdout, "Id:", &release.id)?;
            }
            None => {
                writeln!(stdout)?;
                for release in list.releases.iter().take(5) {
                    writeln!(
                        stdout,
                        "  {}  {}  {}",
                        release.version.clone().unwrap_or_else(|| "?".into()),
                        release.status.clone().unwrap_or_default(),
                        release.created_at.clone().unwrap_or_default()
                    )?;
                }
                if list.releases.is_empty() {
                    log_info!(stdout, "No releases yet. Create one with: forklaunch release create --version <version>");
                }
            }
        }

        Ok(())
    }
}
