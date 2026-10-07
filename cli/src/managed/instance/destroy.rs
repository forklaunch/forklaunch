use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Input, theme::ColorfulTheme};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, delete_text, print_dryrun, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct DestroyCommand;

impl DestroyCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for DestroyCommand {
    fn command(&self) -> Command {
        command(
            "destroy",
            "Permanently destroy a managed instance and its infrastructure",
        )
        .long_about(
            "Permanently destroy a managed instance and its infrastructure.\n\n\
                 This tears down the customer's deployment. It cannot be undone, and no backup\n\
                 is taken automatically first.\n\n\
                 You will be asked to retype the instance id to confirm. Pass --confirm to skip\n\
                 that prompt in scripts and CI — the prompt is never shown when stdin is not a\n\
                 terminal, so a forgotten --confirm fails fast instead of hanging a pipeline.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance to destroy"),
        )
        .arg(
            Arg::new("confirm")
                .long("confirm")
                .help("Skip the confirmation prompt (for CI/scripted use) — this is irreversible")
                .action(ArgAction::SetTrue),
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

        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            return print_dryrun("DELETE", &path, None);
        }

        if !matches.get_flag("confirm") {
            // Refuse rather than prompt when there is no terminal to prompt on. A
            // dialoguer prompt against a closed stdin would hang CI, which the repo has
            // been bitten by before — every flag here must be supplyable non-interactively.
            if !std::io::stdin().is_terminal() {
                bail!(
                    "refusing to destroy instance '{}' without confirmation — stdin is not a terminal, so re-run with --confirm",
                    id
                );
            }

            log_header!(
                stdout,
                Color::Red,
                "This will PERMANENTLY DESTROY managed instance {} and its infrastructure.",
                id
            );
            writeln!(
                stdout,
                "The customer's deployment goes away. This cannot be undone, and no backup is taken first."
            )?;
            writeln!(stdout)?;

            let typed: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Type the instance id ({}) to confirm", id))
                .allow_empty(true)
                .interact_text()
                .with_context(|| "Failed to read confirmation")?;

            if &typed != id {
                bail!(
                    "confirmation did not match '{}' — aborted, nothing was destroyed",
                    id
                );
            }
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let message = delete_text(&path, Missing::Resource(format!("instance '{}'", id)))?;

        log_ok!(
            stdout,
            "Destroy requested for instance {} — {}",
            id,
            message
        );
        log_info!(
            stdout,
            "Teardown runs in the background. Follow it with `forklaunch managed instance list --state destroying`."
        );

        Ok(())
    }
}
