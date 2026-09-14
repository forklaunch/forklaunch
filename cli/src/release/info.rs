use std::io::Write;

use anyhow::{Result, bail};
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

        let releases = fetch_releases(&app, 50)?;

        match matches.get_one::<String>("version") {
            Some(version) => {
                let Some(release) = releases
                    .iter()
                    .find(|r| r.version.as_deref() == Some(version))
                else {
                    bail!(
                        "Release '{}' not found. Known versions: {}",
                        version,
                        releases
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
                for release in releases.iter().take(5) {
                    writeln!(
                        stdout,
                        "  {}  {}  {}",
                        release.version.clone().unwrap_or_else(|| "?".into()),
                        release.status.clone().unwrap_or_default(),
                        release.created_at.clone().unwrap_or_default()
                    )?;
                }
                if releases.is_empty() {
                    log_info!(stdout, "No releases yet. Create one with: forklaunch release create --version <version>");
                }
            }
        }

        Ok(())
    }
}
