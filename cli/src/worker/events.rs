use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

/// `worker events` — what a worker has actually been doing.
///
/// The route existed with no caller on either surface: the dashboard shows a
/// worker's infrastructure and metrics but not its event history, and the CLI
/// could pause/resume/restart a worker without being able to see why it needed
/// it. This is the first thing you want when a worker is misbehaving.
#[derive(Debug)]
pub(crate) struct EventsCommand;

impl EventsCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerEvent {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkerEventsResponse {
    #[serde(default)]
    events: Vec<WorkerEvent>,
    #[serde(default)]
    total: Option<u64>,
}

impl CliCommand for EventsCommand {
    fn command(&self) -> Command {
        command("events", "Show a worker's recent event history")
            .long_about(
                "Show a worker's recent event history — what it processed, what failed, \
                 and how long each took.\n\n\
                 Start here when a worker is misbehaving: `--status failed` narrows to the \
                 failures, and the event type tells you which handler.",
            )
            .arg(
                Arg::new("id")
                    .required(true)
                    .help("Worker id (from `forklaunch app services`)"),
            )
            .arg(
                Arg::new("event_type")
                    .long("event-type")
                    .short('t')
                    .help("Only events of this type"),
            )
            .arg(
                Arg::new("status")
                    .long("status")
                    .short('s')
                    .value_parser(["completed", "failed", "processing"])
                    .help("Only events in this state (filtered client-side)"),
            )
            .arg(
                Arg::new("limit")
                    .long("limit")
                    .short('n')
                    .value_parser(clap::value_parser!(u64).range(1..=500))
                    .default_value("20")
                    .help("How many events to fetch (1-500)"),
            )
            .arg(
                Arg::new("offset")
                    .long("offset")
                    .value_parser(clap::value_parser!(u64))
                    .help("Skip this many events, for paging"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .action(ArgAction::SetTrue)
                    .help("Output raw JSON"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches
            .get_one::<String>("id")
            .context("worker id is required")?;
        let limit = matches.get_one::<u64>("limit").copied().unwrap_or(20);

        let mut url = format!(
            "{}/workers/{}/events?limit={}",
            get_platform_management_api_url(),
            urlencoding::encode(id),
            limit
        );
        if let Some(t) = matches.get_one::<String>("event_type") {
            url.push_str(&format!("&eventType={}", urlencoding::encode(t)));
        }
        if let Some(o) = matches.get_one::<u64>("offset") {
            url.push_str(&format!("&offset={}", o));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if status.as_u16() == 404 {
            bail!(
                "No worker {}. List this application's components with `forklaunch app services`.",
                id
            );
        }
        if !status.is_success() {
            bail!(
                "Failed to read worker events ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if matches.get_flag("json") {
            let raw: serde_json::Value = response
                .json()
                .with_context(|| "Failed to parse worker events response")?;
            writeln!(stdout, "{}", serde_json::to_string_pretty(&raw)?)?;
            return Ok(());
        }

        let parsed: WorkerEventsResponse = response
            .json()
            .with_context(|| "Failed to parse worker events response")?;

        // The route has no status filter, so narrow here rather than pretend it does.
        let wanted = matches.get_one::<String>("status");
        let events: Vec<&WorkerEvent> = parsed
            .events
            .iter()
            .filter(|e| match wanted {
                Some(w) => e.status.as_deref() == Some(w.as_str()),
                None => true,
            })
            .collect();

        writeln!(stdout)?;
        if events.is_empty() {
            match wanted {
                Some(w) => writeln!(stdout, "  No {} events in the last {}.", w, limit)?,
                None => writeln!(stdout, "  No events recorded for this worker.")?,
            }
            writeln!(stdout)?;
            return Ok(());
        }

        for e in &events {
            let state = e.status.as_deref().unwrap_or("-");
            let colour = match state {
                "failed" => Color::Red,
                "completed" => Color::Green,
                "processing" => Color::Yellow,
                _ => Color::White,
            };
            stdout.set_color(ColorSpec::new().set_fg(Some(colour)).set_bold(true))?;
            write!(stdout, "  {:<11}", state)?;
            stdout.reset()?;
            write!(
                stdout,
                "{:<26}{}",
                e.timestamp.as_deref().unwrap_or("-"),
                e.event_type.as_deref().unwrap_or("-")
            )?;
            if let Some(d) = &e.duration {
                write!(stdout, "  ({})", d)?;
            }
            writeln!(stdout)?;
            if let Some(err) = &e.error {
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                writeln!(stdout, "             {}", err)?;
                stdout.reset()?;
            }
        }

        writeln!(stdout)?;
        match (wanted, parsed.total) {
            (Some(w), Some(total)) => writeln!(
                stdout,
                "  {} {} of {} fetched ({} total on the worker)",
                events.len(),
                w,
                parsed.events.len(),
                total
            )?,
            (None, Some(total)) => writeln!(stdout, "  {} of {} total", events.len(), total)?,
            _ => writeln!(stdout, "  {} events", events.len())?,
        }
        writeln!(stdout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        EventsCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn requires_a_worker_id_and_defaults_the_limit() {
        assert!(cmd().try_get_matches_from(["events"]).is_err());
        let m = cmd().try_get_matches_from(["events", "w-1"]).unwrap();
        assert_eq!(*m.get_one::<u64>("limit").unwrap(), 20);
    }

    #[test]
    fn status_is_constrained_and_limit_is_bounded() {
        assert!(
            cmd()
                .try_get_matches_from(["events", "w-1", "--status", "wedged"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["events", "w-1", "--status", "failed"])
                .is_ok()
        );
        assert!(
            cmd()
                .try_get_matches_from(["events", "w-1", "--limit", "0"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["events", "w-1", "--limit", "501"])
                .is_err()
        );
    }
}
