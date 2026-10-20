use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client::{self, make_authenticated_request},
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// `app readiness` — the readiness report's schedule, its runs and its report
/// cards. All of it was dashboard-only, so a scheduled job or an agent could
/// not start a report or read the last one.
///
/// The rails (the scoring configuration) are deliberately not editable here:
/// they are a structured blob with an optimistic-concurrency revision, and
/// hand-editing them from a flag is how you clobber someone else's change.
#[derive(Debug)]
pub(crate) struct ReadinessCommand {
    show: ShowCommand,
    configure: ConfigureCommand,
    run: RunCommand,
    reports: ReportsCommand,
}

impl ReadinessCommand {
    pub(crate) fn new() -> Self {
        Self {
            show: ShowCommand,
            configure: ConfigureCommand,
            run: RunCommand,
            reports: ReportsCommand,
        }
    }
}

impl CliCommand for ReadinessCommand {
    fn command(&self) -> Command {
        command(
            "readiness",
            "Show, schedule or start this application's readiness report",
        )
        .subcommand(self.show.command())
        .subcommand(self.configure.command())
        .subcommand(self.run.command())
        .subcommand(self.reports.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("show", m)) => self.show.handler(m),
            Some(("configure", m)) => self.configure.handler(m),
            Some(("run", m)) => self.run.handler(m),
            Some(("reports", m)) => self.reports.handler(m),
            _ => self.show.handler(matches),
        }
    }
}

fn base_path_arg() -> Arg {
    Arg::new("base_path")
        .short('p')
        .long("path")
        .help("Path to application root (optional)")
}

fn application_id(matches: &ArgMatches) -> Result<String> {
    let _token = require_auth()?;
    let (_app_root, manifest) = require_manifest(matches)?;
    require_integration(&manifest)
}

fn get_json(matches: &ArgMatches, suffix: &str) -> Result<serde_json::Value> {
    let app_id = application_id(matches)?;
    let url = format!(
        "{}/applications/{}/readiness{}",
        get_platform_management_api_url(),
        urlencoding::encode(&app_id),
        suffix
    );
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        bail!(
            "Failed to read readiness ({}): {}",
            response.status(),
            response.text().unwrap_or_default()
        );
    }
    response
        .json()
        .with_context(|| "Failed to parse readiness response")
}

#[derive(Debug)]
struct ShowCommand;
impl CliCommand for ShowCommand {
    fn command(&self) -> Command {
        command("show", "Show the readiness schedule and latest run").arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let body = get_json(matches, "")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ReportsCommand;
impl CliCommand for ReportsCommand {
    fn command(&self) -> Command {
        command("reports", "List readiness report cards").arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let body = get_json(matches, "/reports")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ConfigureCommand;
impl CliCommand for ConfigureCommand {
    fn command(&self) -> Command {
        command("configure", "Set how often the readiness report runs")
            .arg(
                Arg::new("cadence")
                    .long("cadence")
                    .required(true)
                    .help("Schedule cadence (e.g. weekly, monthly, off)"),
            )
            .arg(
                Arg::new("interval_days")
                    .long("interval-days")
                    .value_parser(clap::value_parser!(u64).range(1..))
                    .help("Days between runs, when the cadence takes an interval"),
            )
            .arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let cadence = matches
            .get_one::<String>("cadence")
            .context("cadence is required")?;
        let mut body = serde_json::json!({ "cadence": cadence });
        if let Some(days) = matches.get_one::<u64>("interval_days") {
            body["intervalDays"] = serde_json::Value::from(*days);
        }
        let url = format!(
            "{}/applications/{}/readiness",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response = make_authenticated_request(Method::PUT, &url, Some(body))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to configure readiness ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Scheduled")?;
        stdout.reset()?;
        writeln!(stdout, "  readiness cadence: {}", cadence)?;
        Ok(())
    }
}

#[derive(Debug)]
struct RunCommand;
impl CliCommand for RunCommand {
    fn command(&self) -> Command {
        command("run", "Start a readiness report now")
            .long_about(
                "Start a readiness report now without changing its schedule.\n\n\
                 A report already running answers 409; this reports that as such \
                 rather than starting a second one.",
            )
            .arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let url = format!(
            "{}/applications/{}/readiness/run",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response = http_client::post(&url, serde_json::json!({}))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if status.as_u16() == 409 {
            bail!(
                "A readiness report is already running for this application: {}",
                response.text().unwrap_or_default()
            );
        }
        if !status.is_success() {
            bail!(
                "Failed to start readiness report ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Started")?;
        stdout.reset()?;
        writeln!(stdout, "  readiness report")?;
        writeln!(stdout, "  Results: forklaunch app readiness reports")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        ReadinessCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }

    #[test]
    fn configure_requires_a_cadence() {
        let c = ReadinessCommand::new().command().version("0.0.0-test");
        assert!(
            c.clone()
                .try_get_matches_from(["readiness", "configure"])
                .is_err()
        );
        assert!(
            c.try_get_matches_from(["readiness", "configure", "--cadence", "weekly"])
                .is_ok()
        );
    }
}
