use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Confirm, theme::ColorfulTheme};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::shared::{iam_get, iam_send};
use crate::{CliCommand, core::command::command};

/// The roles the platform's organization model accepts.
const ROLES: [&str; 3] = ["admin", "member", "viewer"];

#[derive(Debug)]
pub(super) struct MembersCommand {
    list: ListCommand,
    invite: InviteCommand,
    role: RoleCommand,
    remove: RemoveCommand,
}

impl MembersCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand,
            invite: InviteCommand,
            role: RoleCommand,
            remove: RemoveCommand,
        }
    }
}

impl CliCommand for MembersCommand {
    fn command(&self) -> Command {
        command(
            "members",
            "List, invite, re-role or remove organization members",
        )
        .subcommand(self.list.command())
        .subcommand(self.invite.command())
        .subcommand(self.role.command())
        .subcommand(self.remove.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", m)) => self.list.handler(m),
            Some(("invite", m)) => self.invite.handler(m),
            Some(("role", m)) => self.role.handler(m),
            Some(("remove", m)) => self.remove.handler(m),
            _ => self.list.handler(matches),
        }
    }
}

#[derive(Debug)]
struct ListCommand;
impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command("list", "List members of your organization").arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Output raw JSON"),
        )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let body = iam_get("/organization/users")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if matches.get_flag("json") {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
            return Ok(());
        }
        let users = body
            .get("users")
            .and_then(|u| u.as_array())
            .or_else(|| body.as_array())
            .cloned()
            .unwrap_or_default();
        writeln!(stdout)?;
        for u in &users {
            let s = |k: &str| u.get(k).and_then(|v| v.as_str()).unwrap_or("-");
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
            write!(stdout, "  {:<34}", s("email"))?;
            stdout.reset()?;
            writeln!(stdout, "{:<10}{}", s("role"), s("id"))?;
        }
        if users.is_empty() {
            writeln!(stdout, "  No members returned.")?;
        }
        writeln!(stdout)?;
        Ok(())
    }
}

#[derive(Debug)]
struct InviteCommand;
impl CliCommand for InviteCommand {
    fn command(&self) -> Command {
        command("invite", "Invite someone to your organization by email")
            .arg(Arg::new("email").required(true).help("Email to invite"))
            .arg(
                Arg::new("role")
                    .long("role")
                    .value_parser(ROLES)
                    .default_value("member")
                    .help("Role to grant: admin, member or viewer"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let email = matches
            .get_one::<String>("email")
            .context("email is required")?;
        let role = matches
            .get_one::<String>("role")
            .context("role has a default")?;
        iam_send(
            Method::POST,
            "/organization/users/invite",
            Some(serde_json::json!({ "email": email, "role": role })),
            "invite this person",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Invited")?;
        stdout.reset()?;
        writeln!(stdout, "  {} as {}", email, role)?;
        writeln!(
            stdout,
            "  Pending invitations: forklaunch org invitations list"
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct RoleCommand;
impl CliCommand for RoleCommand {
    fn command(&self) -> Command {
        command("role", "Change a member's role")
            .arg(
                Arg::new("user_id")
                    .required(true)
                    .help("User id (from `org members list`)"),
            )
            .arg(
                Arg::new("role")
                    .required(true)
                    .value_parser(ROLES)
                    .help("New role: admin, member or viewer"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let user_id = matches
            .get_one::<String>("user_id")
            .context("user id is required")?;
        let role = matches
            .get_one::<String>("role")
            .context("role is required")?;
        iam_send(
            Method::PUT,
            &format!("/organization/users/{}/role", urlencoding::encode(user_id)),
            Some(serde_json::json!({ "role": role })),
            "change this member's role",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Updated")?;
        stdout.reset()?;
        writeln!(stdout, "  {} is now {}", user_id, role)?;
        Ok(())
    }
}

#[derive(Debug)]
struct RemoveCommand;
impl CliCommand for RemoveCommand {
    fn command(&self) -> Command {
        command("remove", "Remove a member from your organization")
            .arg(
                Arg::new("user_id")
                    .required(true)
                    .help("User id (from `org members list`)"),
            )
            .arg(
                Arg::new("yes")
                    .long("yes")
                    .short('y')
                    .action(ArgAction::SetTrue)
                    .help("Skip the confirmation prompt"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let user_id = matches
            .get_one::<String>("user_id")
            .context("user id is required")?;

        if !matches.get_flag("yes") {
            if !std::io::stdin().is_terminal() {
                bail!(
                    "Refusing to remove {} without a terminal. Pass --yes to confirm.",
                    user_id
                );
            }
            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Remove {} from the organization?", user_id))
                .default(false)
                .interact()?;
            if !confirmed {
                let mut stdout = StandardStream::stdout(ColorChoice::Always);
                writeln!(stdout, "Cancelled.")?;
                return Ok(());
            }
        }

        iam_send(
            Method::DELETE,
            &format!("/organization/users/{}", urlencoding::encode(user_id)),
            None,
            "remove this member",
        )?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Removed")?;
        stdout.reset()?;
        writeln!(stdout, "  {}", user_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        MembersCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn invite_defaults_to_member_and_rejects_an_unknown_role() {
        let m = cmd()
            .clone()
            .try_get_matches_from(["members", "invite", "a@b.c"])
            .unwrap();
        let sub = m.subcommand_matches("invite").unwrap();
        assert_eq!(sub.get_one::<String>("role").unwrap(), "member");
        assert!(
            cmd()
                .try_get_matches_from(["members", "invite", "a@b.c", "--role", "owner"])
                .is_err()
        );
    }

    #[test]
    fn role_requires_both_a_user_and_a_valid_role() {
        assert!(
            cmd()
                .clone()
                .try_get_matches_from(["members", "role", "u1"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["members", "role", "u1", "admin"])
                .is_ok()
        );
    }
}
