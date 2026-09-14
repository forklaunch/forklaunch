use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json_public, print_dryrun},
        types::{ManagedInstance, dash},
    },
};

/// `forklaunch managed instance claim`
///
/// The customer-side half of the claim handshake, and the only command in this group
/// that is not operator-facing.
///
/// It is deliberately unauthenticated. The control plane declares this route
/// `access: 'public'` because the person running it is the end customer, who has no
/// ForkLaunch account and never will — the one-time token *is* the credential. Do not
/// add a login requirement here.
#[derive(Debug)]
pub(super) struct ClaimCommand;

impl ClaimCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ClaimCommand {
    fn command(&self) -> Command {
        command(
            "claim",
            "CONSUME a one-time claim link to take ownership of an instance (customer-side, no login)",
        )
        .long_about(
            "Take ownership of a managed instance using the one-time claim link you were given.\n\n\
             THIS IS THE CUSTOMER-SIDE COMMAND, AND IT NEEDS NO FORKLAUNCH ACCOUNT.\n\
             Do not confuse it with `instance claim-link`:\n\
             \x20 `instance claim-link`  REVEALS the link. Operator-side, requires login. Run by\n\
             \x20                        the organization that launched the instance.\n\
             \x20 `instance claim`       CONSUMES the link. Customer-side, requires no login. Run\n\
             \x20                        by the person taking ownership.\n\n\
             ─────────────────────────────────────────────────────────────────────────────\n\
             READ THIS BEFORE YOU RUN IT — YOUR BACKUP PASSPHRASE CANNOT BE RECOVERED\n\
             ─────────────────────────────────────────────────────────────────────────────\n\
             --backup-public-key is an age recipient (a public key) that you derive from a\n\
             passphrase you choose. Your instance's backups are encrypted to it.\n\n\
             The platform stores ONLY THE PUBLIC HALF. It cannot decrypt your backups, and\n\
             neither can ForkLaunch support, now or ever. That is the point of the design —\n\
             but it means the consequence is absolute:\n\n\
             \x20   IF YOU LOSE THE PASSPHRASE, YOUR BACKUPS ARE UNREADABLE. PERMANENTLY.\n\
             \x20   NOBODY CAN RECOVER THEM FOR YOU. THE DATA IS GONE.\n\n\
             Store the passphrase somewhere durable — a password manager, not a terminal\n\
             scrollback — BEFORE running this command. Claiming can only be done once.\n\n\
             The claim also fails as a single indistinguishable error whether the token is\n\
             wrong, expired, already used, or the instance id is unknown. That is deliberate\n\
             on the platform's side, so that the error cannot be used to probe which links\n\
             are still live.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(false)
                .help(
                    "Ignored. The token alone identifies the instance; accepted so older \
                     instructions that pass --id keep working",
                ),
        )
        .arg(
            Arg::new("token")
                .long("token")
                .required(true)
                .help("The one-time claim token (from the claim link you were given)"),
        )
        .arg(
            Arg::new("backup_public_key")
                .long("backup-public-key")
                .required(true)
                .help(
                    "Your age recipient (public key) for backup encryption — derived from a \
                     passphrase only you hold; a lost passphrase means unrecoverable backups",
                ),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it — does NOT consume the claim")
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

        let token = matches
            .get_one::<String>("token")
            .context("--token is required")?;
        let backup_public_key = matches
            .get_one::<String>("backup_public_key")
            .context("--backup-public-key is required")?;

        // No instance id in the path: the platform resolves the instance from the
        // token (POST /managed-mode/claim). Requiring an id would only let a caller
        // probe which ids exist, which is why the id-scoped route was retired.
        let path = "/claim".to_string();
        let body = json!({ "token": token, "backupPublicKey": backup_public_key });

        if matches.get_flag("dryrun") {
            println!(
                "[DRYRUN] this would CONSUME the one-time claim for the instance this token names."
            );
            // The token is the credential. Echoing it into a dry-run transcript that
            // people paste into issues would be the easiest way to leak one, so show the
            // shape of the request without the secret in it.
            let redacted = json!({
                "token": "<redacted — the real token would be sent here>",
                "backupPublicKey": backup_public_key
            });
            return print_dryrun("POST", &path, Some(&redacted));
        }

        // No `resolve_managed_auth` and no `require_managed_mode` preflight here, both on
        // purpose: this route is public, and the summary endpoint the preflight uses is
        // not — preflighting would demand a login this command is specifically designed
        // not to need.
        let instance: ManagedInstance = post_json_public(
            &path,
            body,
            Missing::Custom(
                "this claim link is not valid — it may be mistyped, expired, or already used, \
                 or the instance id may not exist. The platform deliberately does not say which."
                    .to_string(),
            ),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&instance)?);
            return Ok(());
        }

        writeln!(stdout)?;
        log_ok!(stdout, "Instance claimed — it is now yours.");
        writeln!(stdout)?;
        writeln!(stdout, "  Host      {}", dash(&instance.host))?;
        writeln!(stdout, "  Template  {}", dash(&instance.template_slug))?;
        writeln!(stdout, "  Region    {}", dash(&instance.region))?;
        writeln!(stdout, "  State     {}", dash(&instance.state))?;
        if let Some(version) = instance.current_version_semver.as_deref() {
            writeln!(stdout, "  Version   {}", version)?;
        }
        writeln!(stdout)?;

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)).set_bold(true))?;
        writeln!(
            stdout,
            "  Keep your backup passphrase safe. The platform holds only the public half"
        )?;
        writeln!(
            stdout,
            "  of your key and cannot decrypt your backups. If you lose the passphrase,"
        )?;
        writeln!(stdout, "  your backups cannot be recovered by anyone.")?;
        stdout.reset()?;
        writeln!(stdout)?;

        Ok(())
    }
}
