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
        types::RotationAccepted,
    },
};

#[derive(Debug)]
pub(super) struct RotateKeysCommand;

impl RotateKeysCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for RotateKeysCommand {
    fn command(&self) -> Command {
        command(
            "rotate-keys",
            "Issue a new generation of an instance's secrets and redeploy so the app re-encrypts (admin)",
        )
        .long_about(
            "Issue a new generation of an instance's secrets and redeploy so the app re-encrypts.\n\n\
             Every `generated` template variable (the field-encryption key among them) is\n\
             derived afresh for the new generation, every earlier generation is handed to the\n\
             app as LEGACY_<KEY>S, the per-instance gateway HMAC key is minted anew, and the\n\
             current version is redeployed. On startup the app opens what it stored under the\n\
             old key and rewrites it under the new one (Health Vault does this in each\n\
             service's boot). The customer keeps the instance and their data.\n\n\
             Needs the admin role. Allowed while the instance is awaiting_claim, active or\n\
             suspended with no other update pending and no rollout updating it; the 409 names\n\
             the reason (ROTATE_HOST_MISMATCH, ROTATE_NOT_RUNNING, ROTATE_ALREADY_PENDING,\n\
             ROTATE_ROLLOUT_IN_PROGRESS). The host must be echoed with --confirm-host; without\n\
             it you are prompted. Follow it with `instance get`: pendingUpdate=keys clears\n\
             when the deploy lands and the instance answers on its host; a failure keeps the\n\
             marker with lastError, and `instance apply-variables` retries the deploy.\n\n\
             Only rotate a template whose services carry the re-encryption sweep. One that\n\
             ignores LEGACY_ENCRYPTION_KEYS will find all of its encrypted data unreadable.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose keys to rotate"),
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
        let path = format!("/instances/{}/rotate-keys", urlencoding::encode(id));

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
                            "refusing to rotate the keys of instance '{}' without --confirm-host — stdin is not a terminal",
                            id
                        );
                    }
                    log_header!(
                        stdout,
                        Color::Red,
                        "This issues NEW KEYS for managed instance {} and redeploys it.",
                        id
                    );
                    writeln!(
                        stdout,
                        "The app re-encrypts its data under the new key on startup; sign-in via the relay fails until the redeploy lands."
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

        let accepted: RotationAccepted = post_json(
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
            "Key rotation started: generation {} (state {}); redeploying so the app re-encrypts",
            accepted
                .key_generation
                .map(|g| g.to_string())
                .unwrap_or_else(|| "?".to_string()),
            accepted.state.as_deref().unwrap_or("unchanged")
        );
        log_info!(
            stdout,
            "Follow it with `forklaunch managed instance get --id {}`: pendingUpdate=keys clears when the deploy lands and the instance answers. On failure lastError says why and `instance apply-variables --id {}` retries the deploy.",
            id,
            id
        );
        writeln!(stdout)?;
        Ok(())
    }
}
