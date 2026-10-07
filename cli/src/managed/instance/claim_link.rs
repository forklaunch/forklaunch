use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::ClaimLink,
    },
};

#[derive(Debug)]
pub(super) struct ClaimLinkCommand;

impl ClaimLinkCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ClaimLinkCommand {
    fn command(&self) -> Command {
        command(
            "claim-link",
            "REVEAL an instance's one-time claim link (operator-side; destroyed on reveal)",
        )
        .long_about(
            "Reveal an instance's one-time claim link.\n\n\
             *** THIS CAN ONLY BE DONE ONCE. ***\n\n\
             THIS IS THE OPERATOR-SIDE COMMAND, AND IT IS NOT `instance claim`:\n\
             \x20 `claim-link`  REVEALS the link, for you to hand to a customer. Requires login.\n\
             \x20 `claim`       CONSUMES the link, run by the customer. Requires no account.\n\
             Revealing is not claiming — running this does not activate the instance or\n\
             transfer anything. It only shows you the link.\n\n\
             The claim link is what hands ownership of a managed instance to your end\n\
             customer. Revealing it PURGES it from the platform: the link is erased the\n\
             moment it is returned to you, and there is no way to retrieve it again. If you\n\
             lose the value this command prints, the only remedy is to destroy the instance\n\
             and launch a new one.\n\n\
             So: capture the output before you run this in a pipeline you cannot read back,\n\
             and do not run it 'just to check'. Reveal it only when you are ready to hand it\n\
             to the customer.\n\n\
             A claim link only exists while an instance is awaiting_claim and unexpired. If\n\
             the instance is already claimed, its link expired, or it was revealed before,\n\
             this command reports that no claim link is available.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose one-time claim link should be revealed"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it — does NOT consume the claim link")
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
        let path = format!("/instances/{}/claim-link", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            println!(
                "[DRYRUN] this would CONSUME the one-time claim link for instance {}.",
                id
            );
            return print_dryrun("POST", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let json_output = matches.get_flag("json");

        // The platform answers 404 both when the instance does not exist and when it has
        // no claim link left to give (already revealed, expired, or already claimed).
        // Name all of those, because "not found" alone would be misleading for an
        // instance the caller can plainly see in `instance list`.
        let link: ClaimLink = post_json(
            &auth_mode,
            &path,
            json!({}),
            Missing::Resource(format!("a claim link for instance '{}'", id)),
        )?;

        if json_output {
            // Even in --json mode the warning goes to stderr, so it is visible to a human
            // watching a pipeline without corrupting the JSON on stdout.
            eprintln!(
                "[WARN] the claim link for instance {} has now been purged from the platform and cannot be retrieved again.",
                id
            );
            println!("{}", serde_json::to_string_pretty(&link)?);
            return Ok(());
        }

        writeln!(stdout)?;
        log_header!(
            stdout,
            Color::Yellow,
            "ONE-TIME CLAIM LINK — this is the only time it will ever be shown"
        );
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "  {}", link.claim_url)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if let Some(expires_at) = link.expires_at.as_deref() {
            log_info!(stdout, "Expires: {}", expires_at);
        }
        log_warn!(
            stdout,
            "This link has been purged from the platform. Re-running this command will NOT return it."
        );
        log_info!(
            stdout,
            "Copy it now and hand it to the customer. If it is lost, destroy the instance and launch a new one."
        );
        writeln!(stdout)?;

        Ok(())
    }
}
