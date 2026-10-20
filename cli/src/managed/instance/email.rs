use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Confirm, theme::ColorfulTheme};
use reqwest::StatusCode;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::ERROR_FAILED_TO_SEND_REQUEST,
    core::{
        command::command,
        email_sender::{self, EmailSender},
        http_client,
    },
    managed::client::{
        UNSUPPORTED_CONTROL_PLANE, is_unrouted_404, managed_url, require_managed_mode,
        resolve_managed_auth,
    },
};

const DEFAULT_INSTANCE_SENDER: &str = "no-reply@<host prefix>.<platform sending domain>";

fn email_sender_path(id: &str) -> String {
    format!("/instances/{}/email-sender", urlencoding::encode(id))
}

fn id_arg(help: &'static str) -> Arg {
    Arg::new("id").long("id").required(true).help(help)
}

/// Session auth, as every other `managed instance` command uses.
fn require_session() -> Result<()> {
    let auth_mode = resolve_managed_auth()?;
    require_managed_mode(&auth_mode)?;
    Ok(())
}

/// A 404 from a route that was never mounted is an old control plane, not "no sender".
fn unsupported_if_unrouted(body: &str) -> Result<()> {
    if is_unrouted_404(body) {
        bail!("{}", UNSUPPORTED_CONTROL_PLANE);
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct EmailCommand {
    set: EmailSetCommand,
    status: EmailStatusCommand,
    remove: EmailRemoveCommand,
}

impl EmailCommand {
    pub(super) fn new() -> Self {
        Self {
            set: EmailSetCommand,
            status: EmailStatusCommand,
            remove: EmailRemoveCommand,
        }
    }
}

impl CliCommand for EmailCommand {
    fn command(&self) -> Command {
        command(
            "email",
            "Send an instance's managed-gateway email from your own domain (Pro)",
        )
        .long_about(
            "Send a managed instance's email from a domain you own instead of the default \
             no-reply@<host prefix>.<platform sending domain> address (Pro and Enterprise \
             plans).\n\n\
             Until the domain verifies, mail keeps going from the default sender, so sends \
             never fail while you wait. If the domain is delegated to ForkLaunch the DKIM \
             records are written for you; otherwise publish the CNAMEs that `set` and \
             `status` print. A domain can belong to one organization only.\n\n\
             This is separate from `email-gateway`, which sets the instance's sending limits.",
        )
        .subcommand_required(true)
        .subcommand(self.set.command())
        .subcommand(self.status.command())
        .subcommand(self.remove.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("set", m)) => self.set.handler(m),
            Some(("status", m)) => self.status.handler(m),
            Some(("remove", m)) => self.remove.handler(m),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct EmailSetCommand;

impl CliCommand for EmailSetCommand {
    fn command(&self) -> Command {
        command("set", "Send this instance's email from your own domain")
            .arg(id_arg("Id of the instance"))
            .arg(
                Arg::new("domain")
                    .long("domain")
                    .required(true)
                    .help("Domain to send from (e.g. clinic.com)"),
            )
            .arg(
                Arg::new("from")
                    .long("from")
                    .help("Local part of the address, before the @ (e.g. care; default no-reply)"),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .help("Display name shown to recipients (e.g. \"Clinic Care\")"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let domain = matches
            .get_one::<String>("domain")
            .context("--domain is required")?;
        email_sender::validate_domain(domain)?;
        let mut body = serde_json::json!({ "domain": domain });
        if let Some(from) = matches.get_one::<String>("from") {
            email_sender::validate_local_part(from)?;
            body["localPart"] = serde_json::Value::String(from.clone());
        }
        if let Some(name) = matches.get_one::<String>("name") {
            email_sender::validate_display_name(name)?;
            body["displayName"] = serde_json::Value::String(name.clone());
        }

        require_session()?;
        let response = http_client::put(&managed_url(&email_sender_path(id)), body)
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            if status == StatusCode::NOT_FOUND {
                unsupported_if_unrouted(&text)?;
            }
            return Err(email_sender::request_error(
                "set the email sender",
                status,
                &text,
            ));
        }
        let sender: EmailSender = response
            .json()
            .with_context(|| "Failed to parse email sender response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Set")?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  instance {} sends from {}",
            id,
            sender.formatted_from()
        )?;
        email_sender::write_sender(
            &mut stdout,
            &sender,
            &format!("instance {}", id),
            DEFAULT_INSTANCE_SENDER,
        )?;
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Next: run `forklaunch managed instance email status --id {}` to check verification.",
            id
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct EmailStatusCommand;

impl CliCommand for EmailStatusCommand {
    fn command(&self) -> Command {
        command(
            "status",
            "Show the instance's custom email sender and its verification",
        )
        .arg(id_arg("Id of the instance"))
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let json = matches.get_flag("json");

        require_session()?;
        let response = http_client::get(&managed_url(&email_sender_path(id)))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        let text = response.text().unwrap_or_default();

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if status == StatusCode::NO_CONTENT || status == StatusCode::NOT_FOUND {
            unsupported_if_unrouted(&text)?;
            if json {
                writeln!(stdout, "null")?;
            } else {
                writeln!(
                    stdout,
                    "No custom email sender for instance {}: it sends from the default platform sender.",
                    id
                )?;
            }
            return Ok(());
        }
        if !status.is_success() {
            return Err(email_sender::request_error(
                "get the email sender",
                status,
                &text,
            ));
        }
        if json {
            let value: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| "Failed to parse email sender response")?;
            writeln!(stdout, "{}", serde_json::to_string_pretty(&value)?)?;
            return Ok(());
        }
        let sender: EmailSender =
            serde_json::from_str(&text).with_context(|| "Failed to parse email sender response")?;
        email_sender::write_sender(
            &mut stdout,
            &sender,
            &format!("instance {}", id),
            DEFAULT_INSTANCE_SENDER,
        )?;
        writeln!(stdout)?;
        Ok(())
    }
}

#[derive(Debug)]
struct EmailRemoveCommand;

impl CliCommand for EmailRemoveCommand {
    fn command(&self) -> Command {
        command(
            "remove",
            "Go back to the default platform sender for an instance",
        )
        .arg(id_arg("Id of the instance"))
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .action(ArgAction::SetTrue)
                .help("Skip the confirmation prompt"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if !matches.get_flag("yes") {
            if !std::io::stdin().is_terminal() {
                bail!(
                    "Refusing to remove the email sender for instance {} without a terminal. Pass --yes to confirm.",
                    id
                );
            }
            writeln!(
                stdout,
                "Email from instance {} will go back to the default platform sender ({}).",
                id, DEFAULT_INSTANCE_SENDER
            )?;
            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Remove the custom email sender?")
                .default(false)
                .interact()?;
            if !confirmed {
                writeln!(stdout, "Cancelled.")?;
                return Ok(());
            }
        }

        require_session()?;
        let response = http_client::delete(&managed_url(&email_sender_path(id)))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            if status == StatusCode::NOT_FOUND {
                unsupported_if_unrouted(&text)?;
            }
            return Err(email_sender::request_error(
                "remove the email sender",
                status,
                &text,
            ));
        }
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Removed")?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  the custom email sender for instance {}; it sends from the default platform sender again",
            id
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email_cmd() -> Command {
        EmailCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        email_cmd().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(email_cmd().try_get_matches_from(["email"]).is_err());
    }

    #[test]
    fn subcommands_parse() {
        assert!(
            email_cmd()
                .try_get_matches_from([
                    "email",
                    "set",
                    "--id",
                    "inst-1",
                    "--domain",
                    "clinic.com",
                    "--from",
                    "care",
                    "--name",
                    "Clinic Care",
                ])
                .is_ok()
        );
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "status", "--id", "inst-1", "--json"])
                .is_ok()
        );
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "remove", "--id", "inst-1", "-y"])
                .is_ok()
        );
    }

    #[test]
    fn required_args() {
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "set", "--id", "inst-1"])
                .is_err()
        );
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "set", "--domain", "clinic.com"])
                .is_err()
        );
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "status"])
                .is_err()
        );
        assert!(
            email_cmd()
                .try_get_matches_from(["email", "remove", "--yes"])
                .is_err()
        );
    }

    #[test]
    fn path_encodes_the_instance_id() {
        assert_eq!(
            email_sender_path("inst-1"),
            "/instances/inst-1/email-sender"
        );
        assert_eq!(email_sender_path("a/b"), "/instances/a%2Fb/email-sender");
    }
}
