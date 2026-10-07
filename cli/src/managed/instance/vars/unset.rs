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
pub(super) struct UnsetCommand;

impl UnsetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for UnsetCommand {
    fn command(&self) -> Command {
        command("unset", "Clear one instance's value for a custom variable")
            .long_about(
                "Clear one instance's value for a custom variable.\n\n\
                 The template's DECLARATION stays. Only this instance's value goes away, so\n\
                 the variable still appears in `vars list` — as MISSING. If it was declared\n\
                 --required, clearing it means this instance can no longer be provisioned\n\
                 until a new value is supplied.\n\n\
                 To remove the variable everywhere, remove the declaration instead:\n\
                 `forklaunch managed template vars unset --slug <slug> --key <key>`.\n\n\
                 There is no --scope or --service, for the same reason `set` has none: the\n\
                 template's declaration fixed the scoping, and an instance holds one value per\n\
                 key.",
            )
            .arg(
                Arg::new("id")
                    .long("id")
                    .required(true)
                    .help("Id of the instance whose value should be cleared"),
            )
            .arg(
                Arg::new("key")
                    .long("key")
                    .required(true)
                    .help("Name of the custom variable to clear"),
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
        let key = matches
            .get_one::<String>("key")
            .context("--key is required")?;

        let path = format!(
            "/instances/{}/variables/{}",
            urlencoding::encode(id),
            urlencoding::encode(key)
        );

        if matches.get_flag("dryrun") {
            return print_dryrun("DELETE", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        delete_text(
            &path,
            Missing::Resource(format!("value for '{}' on instance '{}'", key, id)),
        )?;

        log_ok!(stdout, "Cleared '{}' for instance {}", key, id);
        log_info!(
            stdout,
            "The template still declares it, so it now shows as MISSING. If it is required, this instance cannot be provisioned until a new value is set."
        );

        Ok(())
    }
}
