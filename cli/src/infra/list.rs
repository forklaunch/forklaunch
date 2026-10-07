use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

use super::resource_resolver::fetch_application_resources;

#[derive(Debug)]
pub(super) struct ListCommand;

impl ListCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command("list", "List provisioned infrastructure resources for an application")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("environment")
                    .short('e')
                    .long("environment")
                    .required(true)
                    .help("Environment to inspect (for example: dev, staging, production)"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .expect("--environment is required")
            .to_string();
        let json_output = matches.get_flag("json");

        let resources = fetch_application_resources(&application_id, &environment)?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&resources.iter().map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.r#type,
                    "serviceName": r.service_name,
                    "environment": r.environment,
                    "region": r.region,
                    "status": r.status,
                })
            }).collect::<Vec<_>>())?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(termcolor::ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Infrastructure resources for {}", environment)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if resources.is_empty() {
            writeln!(stdout, "  No resources provisioned in this environment.")?;
        } else {
            for r in &resources {
                writeln!(
                    stdout,
                    "  {:<24} {:<12} {:<10} {}",
                    r.service_name.as_deref().unwrap_or("?"),
                    r.r#type,
                    r.status,
                    r.region.as_deref().unwrap_or("")
                )?;
            }
        }
        writeln!(stdout)?;

        Ok(())
    }
}
