use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, get_value, require_managed_mode, resolve_managed_auth},
        types::RelayConfig,
    },
};

/// Fetches the product's relay contract. Shared with `set` and `clear`, which print
/// the resulting routes the same way.
pub(super) fn fetch_relay_config(slug: &str) -> Result<RelayConfig> {
    let auth_mode = resolve_managed_auth()?;
    require_managed_mode(&auth_mode)?;
    let path = format!("/templates/{}/relay-config", urlencoding::encode(slug));
    let value = get_value(
        &auth_mode,
        &path,
        Missing::Resource(format!("template '{}'", slug)),
    )?;
    serde_json::from_value(value).context("Failed to parse the relay config")
}

pub(super) fn print_relay_config(slug: &str, config: &RelayConfig) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(stdout, "Relay for template '{}'", slug)?;
    stdout.reset()?;
    writeln!(stdout)?;
    if let Some(url) = &config.callback_url {
        writeln!(stdout, "  Register with the provider:  {}", url)?;
    }
    if let Some(state) = &config.state_format {
        writeln!(stdout, "  Instances mint state as:     {}", state)?;
    }
    writeln!(stdout)?;

    if config.routes.is_empty() {
        writeln!(
            stdout,
            "  No routes declared: the bare callback runs the legacy Epic exchange on the platform."
        )?;
        writeln!(
            stdout,
            "  Declare one with `forklaunch managed template relay set --slug {} --name default --component <service> --path </callback/path> --mode redirect`.",
            slug
        )?;
        writeln!(stdout)?;
        return Ok(());
    }

    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<16} {:<12} {:<24} {:<9} REGISTER THIS URL",
        "ROUTE", "COMPONENT", "PATH", "MODE"
    )?;
    stdout.reset()?;
    for route in &config.routes {
        writeln!(
            stdout,
            "  {:<16} {:<12} {:<24} {:<9} {}",
            route.name,
            route.component,
            route.path,
            route.mode,
            route.callback_url.as_deref().unwrap_or("-"),
        )?;
    }
    writeln!(stdout)?;
    Ok(())
}

#[derive(Debug)]
pub(super) struct ListCommand;

impl ListCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command(
            "list",
            "Show the relay callback URL to register and the routes a template declares",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let config = fetch_relay_config(slug)?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&config)?);
            return Ok(());
        }
        print_relay_config(slug, &config)
    }
}
