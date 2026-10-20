use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::shared::{iam_get, iam_send};
use crate::{CliCommand, core::command::command};

#[derive(Debug)]
pub(super) struct InvitationsCommand {
    list: ListCommand,
    resend: ResendCommand,
    cancel: CancelCommand,
}

impl InvitationsCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand,
            resend: ResendCommand,
            cancel: CancelCommand,
        }
    }
}

impl CliCommand for InvitationsCommand {
    fn command(&self) -> Command {
        command("invitations", "List, resend or cancel pending invitations")
            .subcommand(self.list.command())
            .subcommand(self.resend.command())
            .subcommand(self.cancel.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", m)) => self.list.handler(m),
            Some(("resend", m)) => self.resend.handler(m),
            Some(("cancel", m)) => self.cancel.handler(m),
            _ => self.list.handler(matches),
        }
    }
}

#[derive(Debug)]
struct ListCommand;
impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command("list", "List pending invitations").arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Output raw JSON"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let body = iam_get("/invitations")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if matches.get_flag("json") {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
            return Ok(());
        }
        let rows = body
            .get("invitations")
            .and_then(|v| v.as_array())
            .or_else(|| body.as_array())
            .cloned()
            .unwrap_or_default();
        writeln!(stdout)?;
        for r in &rows {
            let s = |k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or("-");
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
            write!(stdout, "  {:<34}", s("email"))?;
            stdout.reset()?;
            writeln!(stdout, "{:<10}{:<12}{}", s("role"), s("status"), s("id"))?;
        }
        if rows.is_empty() {
            writeln!(stdout, "  No pending invitations.")?;
        }
        writeln!(stdout)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ResendCommand;
impl CliCommand for ResendCommand {
    fn command(&self) -> Command {
        command("resend", "Resend a pending invitation").arg(
            Arg::new("id")
                .required(true)
                .help("Invitation id (from `org invitations list`)"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches.get_one::<String>("id").context("id is required")?;
        iam_send(
            Method::POST,
            &format!("/invitations/{}/resend", urlencoding::encode(id)),
            Some(serde_json::json!({})),
            "resend this invitation",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Resent")?;
        stdout.reset()?;
        writeln!(stdout, "  invitation {}", id)?;
        Ok(())
    }
}

#[derive(Debug)]
struct CancelCommand;
impl CliCommand for CancelCommand {
    fn command(&self) -> Command {
        command("cancel", "Cancel a pending invitation").arg(
            Arg::new("id")
                .required(true)
                .help("Invitation id (from `org invitations list`)"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches.get_one::<String>("id").context("id is required")?;
        iam_send(
            Method::DELETE,
            &format!("/invitations/{}", urlencoding::encode(id)),
            None,
            "cancel this invitation",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Cancelled")?;
        stdout.reset()?;
        writeln!(stdout, "  invitation {}", id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        InvitationsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn resend_and_cancel_both_need_an_id() {
        assert!(
            cmd()
                .clone()
                .try_get_matches_from(["invitations", "resend"])
                .is_err()
        );
        assert!(
            cmd()
                .clone()
                .try_get_matches_from(["invitations", "cancel"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["invitations", "cancel", "inv-1"])
                .is_ok()
        );
    }
}
