use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{command::command, http_client::delete},
};

#[derive(Debug)]
pub(crate) struct DeleteCommand;

impl DeleteCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for DeleteCommand {
    fn command(&self) -> Command {
        command("delete", "Delete an alert rule").arg(
            Arg::new("id")
                .required(true)
                .help("The alert rule ID to delete"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("alert rule id is required")?;

        let url = format!(
            "{}/alert-rules/{}",
            get_observability_api_url(),
            urlencoding::encode(id)
        );
        let response = delete(&url).with_context(|| "Failed to reach observability API")?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to delete alert rule ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Deleted")?;
        stdout.reset()?;
        writeln!(stdout, "  alert rule {}", id)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delete_cmd() -> Command {
        DeleteCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        delete_cmd().debug_assert();
    }

    #[test]
    fn requires_id() {
        assert!(delete_cmd().try_get_matches_from(["delete"]).is_err());
        assert!(
            delete_cmd()
                .try_get_matches_from(["delete", "rule-1"])
                .is_ok()
        );
    }
}
