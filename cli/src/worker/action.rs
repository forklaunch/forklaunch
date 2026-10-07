use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

// pause/resume/restart are identical in shape — one worker id, one POST to a
// fixed sub-path, a plain-text success body — so one struct backs all three.

#[derive(Debug)]
pub(crate) struct ActionCommand {
    name: &'static str,
    about: &'static str,
    path_segment: &'static str,
    past_tense: &'static str,
}

impl ActionCommand {
    pub(crate) fn new(
        name: &'static str,
        about: &'static str,
        path_segment: &'static str,
        past_tense: &'static str,
    ) -> Self {
        Self {
            name,
            about,
            path_segment,
            past_tense,
        }
    }
}

impl CliCommand for ActionCommand {
    fn command(&self) -> Command {
        command(self.name, self.about).arg(
            Arg::new("id")
                .required(true)
                .help("The worker ID to act on"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let worker_id = matches
            .get_one::<String>("id")
            .context("worker id is required")?;

        let url = format!(
            "{}/workers/{}/{}",
            get_platform_management_api_url(),
            urlencoding::encode(worker_id),
            self.path_segment
        );
        let response = http_client::post(&url, serde_json::json!({}))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 404 {
            bail!("Worker '{}' not found.", worker_id);
        }
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to {} worker ({}): {}",
                self.name,
                status,
                response.text().unwrap_or_default()
            );
        }

        let body = response.text().unwrap_or_default();
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Done")?;
        stdout.reset()?;
        if body.trim().is_empty() {
            writeln!(stdout, "  worker {} {}", worker_id, self.past_tense)?;
        } else {
            writeln!(stdout, "  {}", body)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_past_tense_is_not_naive_suffix_concatenation() {
        // Regression: "restart" + "d" = "restartd", not "restarted". Each
        // action carries its own correct past_tense instead of deriving one.
        let restart = ActionCommand::new("restart", "about", "restart", "restarted");
        assert_eq!(restart.past_tense, "restarted");
        let pause = ActionCommand::new("pause", "about", "pause", "paused");
        assert_eq!(pause.past_tense, "paused");
    }
}
