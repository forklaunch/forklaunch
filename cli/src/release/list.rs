use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgMatches, Command};
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::shared::fetch_releases;
use crate::{
    CliCommand,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct ListCommand;

impl ListCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command("list", "List all releases for the current application")
            .arg(
                Arg::new("limit")
                    .long("limit")
                    .default_value("50")
                    .value_parser(clap::value_parser!(u32).range(1..))
                    .help("Maximum number of releases to fetch"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of a formatted table")
                    .action(clap::ArgAction::SetTrue),
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

        let limit = matches.get_one::<u32>("limit").copied().unwrap_or(50);
        let json_output = matches.get_flag("json");

        let releases = fetch_releases(&app, limit)?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&releases)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        if releases.is_empty() {
            log_info!(
                stdout,
                "No releases yet. Create one with: forklaunch release create --version <version>"
            );
            return Ok(());
        }

        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<16}  {:<12}  {:<20}  {:<12}  {:<16}  {}",
            "VERSION", "STATUS", "CREATED", "COMMIT", "BRANCH", "RELEASED BY"
        )?;
        stdout.reset()?;

        for release in &releases {
            let created = release
                .created_at
                .as_deref()
                .map(|c| c.get(..19).unwrap_or(c).to_string())
                .unwrap_or_default();
            let commit = release
                .git_commit
                .as_deref()
                .map(|c| c.chars().take(10).collect::<String>())
                .unwrap_or_default();
            writeln!(
                stdout,
                "  {:<16}  {:<12}  {:<20}  {:<12}  {:<16}  {}",
                release.version.as_deref().unwrap_or("?"),
                release.status.as_deref().unwrap_or(""),
                created,
                commit,
                release.git_branch.as_deref().unwrap_or(""),
                release.released_by.as_deref().unwrap_or("")
            )?;
        }

        writeln!(stdout)?;
        writeln!(stdout, "  {} release(s).", releases.len())?;
        writeln!(stdout)?;

        Ok(())
    }
}
