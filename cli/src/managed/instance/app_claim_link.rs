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
pub(super) struct AppClaimLinkCommand;

impl AppClaimLinkCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for AppClaimLinkCommand {
    fn command(&self) -> Command {
        command(
            "app-claim-link",
            "REVEAL the link the PRODUCT minted for its own claim (one-time; purged on reveal)",
        )
        .long_about(
            "Reveal the claim link the product minted for its own ceremony.\n\n\
             *** THIS CAN ONLY BE DONE ONCE. ***\n\n\
             Handing a customer a managed app happens twice, and this is the SECOND link:\n\
             \x20 `claim-link`      the PLATFORM's. Hands over the infrastructure; the\n\
             \x20                   customer sets a passphrase and the instance is theirs.\n\
             \x20 `app-claim-link`  the PRODUCT's. Hands over the product itself — an admin\n\
             \x20                   account, a verified phone, whatever owning it means.\n\n\
             The product mints this one. When its template declares a claim hook, the\n\
             platform asks the instance for the link as soon as the platform claim commits,\n\
             so it is usually waiting by the time you run this. Before the hook existed an\n\
             operator minted it by hand, with a signing key pulled out of the instance's\n\
             configuration — that path still works for a product with no hook.\n\n\
             Revealing PURGES it, exactly like the platform's own link: capture the output.\n\
             A reset clears it too, so a recycled instance never hands the next owner the\n\
             previous owner's way in.\n\n\
             No link available means one of three things, and the command cannot tell them\n\
             apart: it was already revealed or has expired; the product has not minted yet\n\
             (the hook runs just after the claim — try again shortly); or this template\n\
             declares no claim hook, in which case mint it by hand.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose product claim link should be revealed"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it — does NOT consume the link")
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
        let path = format!("/instances/{}/app-claim-link", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            println!(
                "[DRYRUN] this would CONSUME the product's one-time claim link for instance {}.",
                id
            );
            return print_dryrun("POST", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let json_output = matches.get_flag("json");

        let link: ClaimLink = post_json(
            &auth_mode,
            &path,
            json!({}),
            Missing::Resource(format!(
                "a product claim link for instance '{}' — already revealed or expired, not minted yet, or this template declares no claim hook",
                id
            )),
        )?;

        if json_output {
            eprintln!(
                "[WARN] the product claim link for instance {} has now been purged and cannot be retrieved again.",
                id
            );
            println!("{}", serde_json::to_string_pretty(&link)?);
            return Ok(());
        }

        writeln!(stdout)?;
        log_header!(
            stdout,
            Color::Yellow,
            "ONE-TIME PRODUCT CLAIM LINK — this is the only time it will ever be shown"
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
            "Send it to the customer to finish setting the product up."
        );
        writeln!(stdout)?;

        Ok(())
    }
}
