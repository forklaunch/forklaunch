use std::io::Write;

use anyhow::Result;
use clap::{ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::shared::iam_send;
use crate::{CliCommand, core::command::command};

/// `org domain` — DNS-TXT verification of the organization's email domain,
/// and the auto-join switch that verification unlocks.
#[derive(Debug)]
pub(super) struct DomainCommand {
    verify: VerifyCommand,
    check: CheckCommand,
    auto_join: AutoJoinCommand,
}

impl DomainCommand {
    pub(super) fn new() -> Self {
        Self {
            verify: VerifyCommand,
            check: CheckCommand,
            auto_join: AutoJoinCommand,
        }
    }
}

impl CliCommand for DomainCommand {
    fn command(&self) -> Command {
        command(
            "domain",
            "Verify your organization's email domain and control auto-join",
        )
        .subcommand_required(true)
        .subcommand(self.verify.command())
        .subcommand(self.check.command())
        .subcommand(self.auto_join.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("verify", m)) => self.verify.handler(m),
            Some(("check", m)) => self.check.handler(m),
            Some(("auto-join", m)) => self.auto_join.handler(m),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct VerifyCommand;
impl CliCommand for VerifyCommand {
    fn command(&self) -> Command {
        command(
            "verify",
            "Start DNS TXT verification and print the record to publish",
        )
    }
    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let response = iam_send(
            Method::POST,
            "/organization/my-organization/verify-domain",
            Some(serde_json::json!({})),
            "start domain verification",
        )?;
        let body: serde_json::Value = response.json().unwrap_or(serde_json::json!({}));
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Started")?;
        stdout.reset()?;
        writeln!(stdout, "  domain verification")?;
        writeln!(stdout)?;
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Publish the TXT record above, then: forklaunch org domain check"
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct CheckCommand;
impl CliCommand for CheckCommand {
    fn command(&self) -> Command {
        command("check", "Check whether the TXT record has propagated")
    }
    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let response = iam_send(
            Method::POST,
            "/organization/my-organization/check-domain-verification",
            Some(serde_json::json!({})),
            "check domain verification",
        )?;
        let body: serde_json::Value = response.json().unwrap_or(serde_json::json!({}));
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
        Ok(())
    }
}

#[derive(Debug)]
struct AutoJoinCommand;
impl CliCommand for AutoJoinCommand {
    fn command(&self) -> Command {
        command(
            "auto-join",
            "Let anyone with a verified-domain email join automatically",
        )
        .arg(
            clap::Arg::new("state")
                .required(true)
                .value_parser(["on", "off"])
                .help("on or off"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let enabled = matches.get_one::<String>("state").map(|s| s == "on") == Some(true);
        iam_send(
            Method::PUT,
            "/organization/my-organization/domain-auto-join",
            Some(serde_json::json!({ "enabled": enabled })),
            "change domain auto-join",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Updated")?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  domain auto-join is {}",
            if enabled { "on" } else { "off" }
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        DomainCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn auto_join_only_accepts_on_or_off() {
        assert!(
            cmd()
                .clone()
                .try_get_matches_from(["domain", "auto-join"])
                .is_err()
        );
        assert!(
            cmd()
                .clone()
                .try_get_matches_from(["domain", "auto-join", "maybe"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["domain", "auto-join", "on"])
                .is_ok()
        );
    }
}
