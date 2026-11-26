use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::shared::{iam_get, iam_send};
use crate::{CliCommand, core::command::command};

#[derive(Debug)]
pub(super) struct ShowCommand;

impl ShowCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ShowCommand {
    fn command(&self) -> Command {
        command("show", "Show your organization, or rename it")
            .arg(
                Arg::new("name")
                    .long("name")
                    .help("Rename the organization"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Output raw JSON"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        if let Some(name) = matches.get_one::<String>("name") {
            iam_send(
                Method::PUT,
                "/organization/my-organization",
                Some(serde_json::json!({ "name": name })),
                "rename the organization",
            )?;
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
            write!(stdout, "  Renamed")?;
            stdout.reset()?;
            writeln!(stdout, "  organization to {}", name)?;
            return Ok(());
        }

        let org = iam_get("/organization/my-organization")?;
        if matches.get_flag("json") {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&org)?)?;
            return Ok(());
        }

        let field = |k: &str| {
            org.get(k)
                .and_then(|v| {
                    if v.is_string() {
                        v.as_str().map(str::to_string)
                    } else if v.is_null() {
                        None
                    } else {
                        Some(v.to_string())
                    }
                })
                .unwrap_or_else(|| "-".to_string())
        };

        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "  {}", field("name"))?;
        stdout.reset()?;
        writeln!(stdout, "  id:            {}", field("id"))?;
        writeln!(stdout, "  domain:        {}", field("domain"))?;
        writeln!(stdout, "  region:        {}", field("primaryRegion"))?;
        writeln!(stdout, "  team size:     {}", field("teamSize"))?;
        writeln!(stdout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        ShowCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }
}
