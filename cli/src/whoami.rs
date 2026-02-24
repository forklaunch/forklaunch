use std::io::Write;

use anyhow::Result;
use clap::{ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream};

use crate::{
    CliCommand,
    core::command::command,
};

#[derive(Debug)]
pub(super) struct WhoAmICommand;

impl WhoAmICommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for WhoAmICommand {
    fn command(&self) -> Command {
        command("whoami", "Get the current user")
    }

    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let token = crate::core::validate::require_auth()?;
        // TODO: parse token and get all relevant user information
        writeln!(stdout, "{}", token)?;
        Ok(())
    }
}
