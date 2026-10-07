use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, get_value, print_dryrun, require_managed_mode, resolve_managed_auth},
        rollout::print_rollout,
        types::FleetRollout,
    },
};

#[derive(Debug)]
pub(super) struct GetCommand;

impl GetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for GetCommand {
    fn command(&self) -> Command {
        command(
            "get",
            "Show a rollout and the state of every instance in it",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the rollout"),
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
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/rollouts/{}", urlencoding::encode(id));
        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("rollout '{}'", id)),
        )?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(());
        }
        let rollout: FleetRollout =
            serde_json::from_value(value).context("unexpected rollout shape")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        print_rollout(&mut stdout, &rollout)
    }
}
