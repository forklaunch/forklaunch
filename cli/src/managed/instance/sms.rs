use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, extract_list, get_value, require_managed_mode, resolve_managed_auth},
        types::{SmsDispatch, dash},
    },
};

/// Truncates a provider error so one long refusal cannot wrap the whole table into
/// unreadability. The full text is one `--json` away.
fn short(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{}…", kept)
}

#[derive(Debug)]
pub(super) struct SmsCommand;

impl SmsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for SmsCommand {
    fn command(&self) -> Command {
        command(
            "sms",
            "Show what happened to the text messages the platform sent for an instance",
        )
        .long_about(
            "Show what happened to the text messages the platform sent for an instance.\n\n\
             When a customer says they never got their code, the answer is almost always\n\
             here, and until this command existed it was only in the platform's logs. Each\n\
             row is one attempt: which message it was, whether the provider accepted it, the\n\
             provider's message id for chasing it up on their side, and — when the provider\n\
             refused — its own reason, verbatim.\n\n\
             A refusal is usually about the sending number rather than the customer: an\n\
             unverified toll-free number, a destination the carrier blocks, or a number the\n\
             provider will not format. Those are fixed with the provider, not by resending.\n\n\
             THE MESSAGE BODY IS NEVER SHOWN, and the control plane does not send it. A\n\
             claim message contains the claim link, so printing bodies here would be a\n\
             second way to reveal a one-time link — one with no record that it happened.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose SMS attempts should be listed"),
        )
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

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let path = format!("/instances/{}/sms", urlencoding::encode(id));
        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;
        let dispatches: Vec<SmsDispatch> = extract_list(value, &["dispatches", "sms"])?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&dispatches)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "SMS attempts for instance {}", id)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if dispatches.is_empty() {
            // "Nothing was sent" and "something was sent and failed" are different
            // problems with different fixes, so say which this is rather than printing an
            // empty table and leaving the reader to guess.
            writeln!(
                stdout,
                "  No SMS has been attempted for this instance. Nothing was sent — so nothing was"
            )?;
            writeln!(
                stdout,
                "  blocked or rejected. If a customer is waiting on a code, the send was never"
            )?;
            writeln!(stdout, "  triggered.")?;
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<24} {:<18} {:<10} {:<36} REASON",
            "SENT", "PURPOSE", "STATUS", "PROVIDER MESSAGE ID"
        )?;
        stdout.reset()?;

        let mut failures = 0usize;
        for dispatch in &dispatches {
            let status = dispatch.status.as_deref().unwrap_or("-");
            let failed = matches!(status, "failed" | "error" | "rejected" | "undelivered");
            if failed {
                failures += 1;
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
            }
            writeln!(
                stdout,
                "  {:<24} {:<18} {:<10} {:<36} {}",
                dash(&dispatch.created_at),
                dash(&dispatch.purpose),
                status,
                dash(&dispatch.provider_message_id),
                short(dispatch.error.as_deref().unwrap_or("-"), 60)
            )?;
            if failed {
                stdout.reset()?;
            }
        }
        writeln!(stdout)?;

        if failures > 0 {
            log_info!(
                stdout,
                "The REASON column is the provider's own words. Full text: re-run with --json."
            );
            writeln!(stdout)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_provider_error_is_truncated_rather_than_wrapping_the_table() {
        let long = "The 'To' number is not currently reachable via SMS because the sending toll-free number has not completed verification";
        let rendered = short(long, 60);
        assert_eq!(rendered.chars().count(), 60);
        assert!(rendered.ends_with('…'));
    }

    #[test]
    fn an_error_that_already_fits_is_left_exactly_as_the_provider_wrote_it() {
        assert_eq!(
            short("Invalid 'To' phone number", 60),
            "Invalid 'To' phone number"
        );
    }
}
