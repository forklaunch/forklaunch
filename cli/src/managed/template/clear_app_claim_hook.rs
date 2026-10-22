use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, delete_text, print_dryrun, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct ClearAppClaimHookCommand;

impl ClearAppClaimHookCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ClearAppClaimHookCommand {
    fn command(&self) -> Command {
        command(
            "clear-app-claim-hook",
            "Stop asking this product to mint its own sign-up link",
        )
        .long_about(
            "Stop asking this product to mint its own sign-up link.\n\n\
             With a hook declared, the platform calls the product the moment a customer\n\
             claims an instance and holds the link it returns. Clearing it means new\n\
             instances get the platform claim only, and whoever operates the product hands\n\
             out the first login some other way.\n\n\
             THIS IS A SEPARATE COMMAND RATHER THAN A FLAG ON `update`, and the reason is\n\
             worth stating: the PATCH's documentation used to say passing `null` withdrew\n\
             the hook, while the schema rejected null outright — an object field has no\n\
             nullable form in this validator. So the documented way to turn a hook off did\n\
             not exist, and nobody found out until they tried. Withdrawing something is an\n\
             action, and it gets a verb.\n\n\
             This affects instances launched AFTERWARDS. An instance that already holds a\n\
             minted link keeps it, and clearing the hook does not revoke a link already\n\
             given to a customer.",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template whose app claim hook should be withdrawn"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let path = format!("/templates/{}/app-claim-hook", urlencoding::encode(slug));

        if matches.get_flag("dryrun") {
            return print_dryrun("DELETE", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        delete_text(&path, Missing::Resource(format!("template '{}'", slug)))?;

        writeln!(stdout)?;
        log_ok!(
            stdout,
            "Template '{}' no longer declares an app claim hook.",
            slug
        );
        log_info!(
            stdout,
            "Instances launched from now on get the platform claim link only. Declare it again with: forklaunch managed template update --slug {} --app-claim-hook <component>:<path>",
            slug
        );
        writeln!(stdout)?;

        Ok(())
    }
}
