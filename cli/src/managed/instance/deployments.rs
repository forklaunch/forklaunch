use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, extract_list, get_value, print_dryrun, require_managed_mode,
            resolve_managed_auth,
        },
        types::{InstanceDeployment, dash},
    },
};

#[derive(Debug)]
pub(super) struct DeploymentsCommand;

impl DeploymentsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for DeploymentsCommand {
    fn command(&self) -> Command {
        command(
            "deployments",
            "List the deployments of an instance's backing application",
        )
        .long_about(
            "List the deployments of an instance's backing application, newest first.\n\n\
             Every launch, resize, variable push, rollout and reset is one deployment here.\n\
             Use it to follow a lifecycle request to completion when you need progress rather\n\
             than the final state: each row carries the platform's status and, on failure,\n\
             its error message. Open the full log with `forklaunch deploy info -d <id>` or on\n\
             the dashboard's deployment page.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
        )
        .arg(
            Arg::new("limit")
                .long("limit")
                .default_value("25")
                .value_parser(clap::value_parser!(u16).range(1..=100))
                .help("How many deployments to show (1-100)"),
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
        let limit = matches.get_one::<u16>("limit").copied().unwrap_or(25);
        let path = format!(
            "/instances/{}/deployments?limit={}",
            urlencoding::encode(id),
            limit
        );

        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;
        let deployments: Vec<InstanceDeployment> = extract_list(value, &["deployments"])?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&deployments)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Deployments of instance {}", id)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if deployments.is_empty() {
            writeln!(stdout, "  No deployments recorded yet.")?;
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<38} {:<18} {:<10} {:<26} {}",
            "DEPLOYMENT", "STATUS", "VERSION", "STARTED", "BY"
        )?;
        stdout.reset()?;
        for deployment in &deployments {
            writeln!(
                stdout,
                "  {:<38} {:<18} {:<10} {:<26} {}",
                dash(&deployment.id),
                dash(&deployment.status),
                dash(&deployment.release_version),
                dash(&deployment.created_at),
                dash(&deployment.deployed_by),
            )?;
            if let Some(error) = deployment.error_message.as_deref() {
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                writeln!(stdout, "      {}", error)?;
                stdout.reset()?;
            }
        }
        writeln!(stdout)?;
        Ok(())
    }
}
