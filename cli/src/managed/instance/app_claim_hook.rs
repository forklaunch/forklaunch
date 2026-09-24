use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::HookEnqueued,
    },
};

#[derive(Debug)]
pub(super) struct AppClaimHookCommand;

impl AppClaimHookCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for AppClaimHookCommand {
    fn command(&self) -> Command {
        command(
            "app-claim-hook",
            "Ask the product to mint its sign-up link again, after the first attempt failed",
        )
        .long_about(
            "Ask the running app to mint its own sign-up link again.\n\n\
             The platform calls this hook once, automatically, the moment an instance's\n\
             platform claim commits. If the app was still starting, or the call timed out,\n\
             or it answered an error, the attempt is recorded against the instance and\n\
             nothing retries it. Before this command existed the only way to get a second\n\
             attempt was to DESTROY and relaunch the instance — throwing away a claimed\n\
             deployment to repeat one HTTP call.\n\n\
             SAFE TO REPEAT. The platform refuses to mint a second link while one is\n\
             already held, and refuses entirely once the product reports itself claimed, so\n\
             running this when it was not needed changes nothing.\n\n\
             This queues the work; it does not wait for it. Read the outcome with\n\
             `instance app-claim-link --id <id>` — on success that reveals the link, and on\n\
             failure it reports what the app said.\n\n\
             If the hook keeps failing, the usual causes are the template pointing at a\n\
             component or path the built app does not serve (check with\n\
             `template update --slug <slug> --app-claim-hook <component>:<path>`), or the\n\
             app rejecting the platform's signature. Neither is fixed by retrying.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose app claim hook should be run again"),
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
        let path = format!("/instances/{}/app-claim-hook", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let result: HookEnqueued = post_json(
            &auth_mode,
            &path,
            json!({}),
            Missing::Resource(format!("instance '{}'", id)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        writeln!(stdout)?;
        // `enqueued: false` is not a shape the control plane returns today — it answers
        // 202 with `true` or fails outright. Reporting it honestly anyway costs one
        // branch, and beats printing "queued" for an answer that said otherwise.
        if result.enqueued == Some(false) {
            log_warn!(
                stdout,
                "The control plane accepted the request but did not queue the hook for instance {}.",
                id
            );
        } else {
            log_ok!(
                stdout,
                "Queued another claim-hook attempt for instance {}.",
                id
            );
        }
        log_info!(
            stdout,
            "This does not wait for the app to answer. Read the outcome with: forklaunch managed instance app-claim-link --id {}",
            id
        );
        writeln!(stdout)?;

        Ok(())
    }
}
