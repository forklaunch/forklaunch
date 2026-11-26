use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Input, theme::ColorfulTheme};
use serde_json::json;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::StateAccepted,
    },
};

#[derive(Debug)]
pub(super) struct ResetCommand;

impl ResetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ResetCommand {
    fn command(&self) -> Command {
        command(
            "reset",
            "Erase an instance's data and return it to the claimable pool (admin)",
        )
        .long_about(
            "Erase an instance's data and return it to the claimable pool.\n\n\
             Every database and the cache are wiped, the signing key is rotated, the current\n\
             owner's claim is removed, and the instance comes back as awaiting_claim with a\n\
             fresh one-time claim link. The previous owner's backups stay encrypted to a key\n\
             only they hold; nothing here can read them. This is the ONLY road back to\n\
             awaiting_claim, and it needs the admin role.\n\n\
             Allowed from awaiting_claim, active, suspended and — to retry a reset that\n\
             failed — provisioning_failed. Refused while a fleet rollout is updating the\n\
             instance (409 RESET_ROLLOUT_IN_PROGRESS) and for a provisioning_failed instance\n\
             that never launched (409 RESET_NOTHING_TO_RESET: retry the launch with\n\
             `instance resume`, or destroy it).\n\n\
             The instance's host must be echoed with --confirm-host; without it you are\n\
             prompted to type it. Completion is by evidence: the platform's deployment\n\
             callback plus a live health probe, about 3-4 minutes on a shared pool. Poll\n\
             `instance get` until the state is awaiting_claim, then `instance claim-link`.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance to reset"),
        )
        .arg(
            Arg::new("confirm_host")
                .long("confirm-host")
                .help("The instance's host, echoed to confirm (required non-interactively)"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}/reset", urlencoding::encode(id));

        let confirm_host = match matches.get_one::<String>("confirm_host") {
            Some(host) => host.trim().to_string(),
            None => {
                if matches.get_flag("dryrun") {
                    "<host>".to_string()
                } else {
                    // Refuse rather than prompt when there is no terminal to prompt on: a
                    // prompt against a closed stdin hangs CI.
                    if !std::io::stdin().is_terminal() {
                        bail!(
                            "refusing to reset instance '{}' without --confirm-host — stdin is not a terminal",
                            id
                        );
                    }
                    log_header!(
                        stdout,
                        Color::Red,
                        "This ERASES ALL DATA of managed instance {} and hands it to the next customer.",
                        id
                    );
                    writeln!(
                        stdout,
                        "Databases and cache are wiped, the key is rotated, the current owner's claim is removed."
                    )?;
                    writeln!(stdout)?;
                    Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Type the instance's host to confirm")
                        .allow_empty(false)
                        .interact_text()
                        .with_context(|| "Failed to read confirmation")?
                }
            }
        };

        let body = json!({ "confirmHost": confirm_host });
        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let accepted: StateAccepted = post_json(
            &auth_mode,
            &path,
            body,
            Missing::Resource(format!("instance '{}'", id)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&accepted)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "Reset started (state {}): wiping data, rotating the key, returning to the pool",
            accepted.state.as_deref().unwrap_or("resetting")
        );
        log_info!(
            stdout,
            "Poll `forklaunch managed instance get --id {}` until it reads awaiting_claim (3-4 minutes), then reveal the new link with `instance claim-link --id {}`.",
            id,
            id
        );
        log_info!(
            stdout,
            "If it lands in provisioning_failed, lastError says why; the same command retries it."
        );
        writeln!(stdout)?;
        Ok(())
    }
}
