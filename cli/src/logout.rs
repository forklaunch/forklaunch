use std::{fs::remove_file, io::Write};

use anyhow::Result;
use clap::{ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{command::command, token::get_token_path},
};

pub(super) struct LogoutCommand;

impl LogoutCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for LogoutCommand {
    fn command(&self) -> Command {
        command("logout", "Log out of the forklaunch platform")
    }

    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        // TODO: call logout API and serialize to ~/.forklaunch/token
        let token_path = get_token_path()?;
        remove_file(token_path)?;

        log_ok!(stdout, "Successfully logged out!");
        Ok(())
    }
}
