//! `forklaunch app compute`: whether each service sleeps when idle or runs all
//! the time.
//!
//! A service runs one of two ways. **Scale to zero**: it is suspended after a
//! few minutes without a request and woken by the next one, which takes about
//! a second; you pay for the time it is awake. **Always on**: it runs all the
//! time and never makes a request wait.
//!
//! Say an app has an `api` that is busy all day and an `admin` service used an
//! hour a week. `admin` belongs on scale to zero; `api` on always on.
//!
//! Every service is on `auto` unless someone says otherwise: the platform runs
//! it scale to zero when it can run that way, and always on when it cannot (it
//! serves WebSockets, or the application declares a compliance framework, for
//! example). Scale to zero is the default; always on is the exception, and it
//! carries a reason (`--reason`) so the next person knows why the service does
//! not sleep. A service another service calls every minute, for instance,
//! would never sleep and is cheaper kept always on.
//!
//! With no `--service`, this command shows each service's mode. With one or
//! more, it records the change, which takes effect on the next deployment.
//! Workers are not listed: nothing calls a worker, so nothing could wake it.
//!
//! Under a service that runs always on, the listing gives the reason and says
//! where it came from when no person wrote it: a platform rule, or what the
//! last deploy found. Where the platform could read the last week of traffic it
//! also shows the share of time the service was awake (or had requests) and
//! what that suggests. That is advice: only `--service` changes a mode.

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// The modes, in the order they are offered: the default first.
pub(crate) const COMPUTE_MODES: &[&str] = &["auto", "scale-to-zero", "always-on"];

fn parse_service_modes(entries: &[&String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut modes = serde_json::Map::new();
    for entry in entries {
        let (service, mode) = entry
            .split_once('=')
            .with_context(|| format!("--service '{}' must be <service>=<mode>", entry))?;
        if service.is_empty() || mode.is_empty() {
            bail!(
                "--service '{}' must contain a non-empty service and mode",
                entry
            );
        }
        if modes.contains_key(service) {
            bail!("--service specified more than once for '{}'", service);
        }
        if !COMPUTE_MODES.contains(&mode) {
            bail!(
                "--service '{}': '{}' is not a compute mode (one of {})",
                entry,
                mode,
                COMPUTE_MODES.join(", ")
            );
        }
        modes.insert(
            service.to_string(),
            serde_json::Value::String(mode.to_string()),
        );
    }
    Ok(modes)
}

/// The reason, attached to each service this command sets to always-on. A
/// reason with nothing to attach to is a mistake worth saying out loud.
fn always_on_reasons(
    modes: &serde_json::Map<String, serde_json::Value>,
    reason: Option<&String>,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut reasons = serde_json::Map::new();
    let Some(reason) = reason.map(|r| r.trim()).filter(|r| !r.is_empty()) else {
        return Ok(reasons);
    };
    for (service, mode) in modes {
        if mode == "always-on" {
            reasons.insert(
                service.clone(),
                serde_json::Value::String(reason.to_string()),
            );
        }
    }
    if reasons.is_empty() {
        bail!("--reason explains always-on, and no service here is being set to always-on");
    }
    Ok(reasons)
}

#[derive(Debug)]
pub(crate) struct ComputeCommand;

impl ComputeCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ComputeCommand {
    fn command(&self) -> Command {
        command(
            "compute",
            "Show or set whether each service scales to zero when idle or is always on",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment to show or change"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region to show or change"),
        )
        .arg(
            Arg::new("service")
                .long("service")
                .action(ArgAction::Append)
                .help(
                    "<service>=<mode>, repeatable. Modes: scale-to-zero (suspended when idle, \
                     woken by the next request in about a second), always-on (never waits), \
                     auto (scale to zero whenever the service can run that way; what a new \
                     service starts on). Takes effect on the next deployment \
                     (e.g. --service admin=scale-to-zero --service api=always-on)",
                ),
        )
        .arg(Arg::new("reason").long("reason").requires("service").help(
            "Why the services set to always-on here do not sleep; shown beside them \
                     from then on (e.g. --reason \"called every minute by the publisher\")",
        ))
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let api_url = get_platform_management_api_url();
        let changes: Vec<&String> = matches
            .get_many::<String>("service")
            .map(|values| values.collect())
            .unwrap_or_default();

        let response = if changes.is_empty() {
            http_client::get(&format!(
                "{}/applications/{}/compute?environment={}&region={}",
                api_url, application_id, environment, region
            ))
        } else {
            let modes = parse_service_modes(&changes)?;
            let reasons = always_on_reasons(&modes, matches.get_one::<String>("reason"))?;
            http_client::patch(
                &format!("{}/applications/{}/compute", api_url, application_id),
                serde_json::json!({
                    "environment": environment,
                    "region": region,
                    "services": modes,
                    "reasons": reasons,
                }),
            )
        }
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to {} compute ({}): {}",
                if changes.is_empty() { "read" } else { "set" },
                response.status(),
                response.text().unwrap_or_default()
            );
        }
        let compute: ComputeResponse = response
            .json()
            .with_context(|| "Failed to parse compute response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        render(&mut stdout, &compute)?;
        if !changes.is_empty() {
            writeln!(stdout)?;
            writeln!(
                stdout,
                "  Deploy to apply: forklaunch deploy create -e {} --region {}",
                environment, region
            )?;
        }
        Ok(())
    }
}

/// The widest line the listing prints.
const LINE_WIDTH: usize = 100;

/// `text` broken at spaces into lines of at most `width` characters. A word
/// longer than that gets a line to itself.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The services below the heading, one string per line.
fn service_lines(services: &[ServiceCompute]) -> Vec<String> {
    let width = services
        .iter()
        .map(|service| service.service.len())
        .max()
        .unwrap_or(0);
    let indent = " ".repeat(4 + width + 4);
    let room = LINE_WIDTH.saturating_sub(indent.len()).max(40);
    let mut lines = Vec::new();
    for service in services {
        lines.push(format!(
            "    {:<width$}  {}",
            service.service,
            service.summary(),
            width = width
        ));
        for detail in service.details() {
            for line in wrap(&detail, room) {
                lines.push(format!("{}{}", indent, line));
            }
        }
    }
    lines
}

fn render(stdout: &mut StandardStream, compute: &ComputeResponse) -> Result<()> {
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    write!(
        stdout,
        "  Compute: {} ({})",
        compute.environment, compute.region
    )?;
    stdout.reset()?;
    writeln!(stdout)?;
    for line in service_lines(&compute.services) {
        writeln!(stdout, "{}", line)?;
    }
    if let Some(message) = &compute.message {
        writeln!(stdout, "  {}", message)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputeResponse {
    environment: String,
    region: String,
    #[serde(default)]
    services: Vec<ServiceCompute>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceCompute {
    service: String,
    /// What is stored: auto, scale-to-zero or always-on.
    mode: String,
    /// Where it runs after its last deploy, or would on the next.
    runs_as: String,
    /// Why the service runs always-on: what stopped auto, or the reason kept
    /// with always-on.
    #[serde(default)]
    reason: Option<String>,
    /// Who gave the reason: `owner` (a person wrote it), `platform` (a rule
    /// that applies today) or `deploy` (the last deploy found the service
    /// cannot run scale to zero). Absent on an older platform.
    #[serde(default)]
    reason_from: Option<String>,
    /// What the last week of traffic shows. Absent when it cannot be read,
    /// and on an older platform.
    #[serde(default)]
    observed: Option<ObservedCompute>,
}

/// What a service's traffic says about where it should run. Every field is
/// optional so that a platform that sends fewer still parses.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservedCompute {
    /// `awake` (share of time a scale-to-zero service was awake) or
    /// `busy-windows` (share of five-minute windows with a request).
    #[serde(default)]
    measure: Option<String>,
    #[serde(default)]
    fraction: Option<f64>,
    #[serde(default)]
    window_start: Option<String>,
    #[serde(default)]
    window_end: Option<String>,
    /// Where the figure was read from.
    #[serde(default)]
    source: Option<String>,
    /// `all`, or `external-only` when calls from the application's own
    /// services and workers are not counted.
    #[serde(default)]
    scope: Option<String>,
    /// The advice in words. The `advice` code beside it is not read.
    #[serde(default)]
    recommendation: Option<String>,
}

/// A share as the platform words it: "40%", or "under 1%" for a trace.
fn percent(fraction: f64) -> String {
    if fraction > 0.0 && fraction < 0.01 {
        "under 1%".to_string()
    } else {
        format!("{}%", (fraction * 100.0).round())
    }
}

/// The day of an ISO timestamp: "2026-11-08T00:00:00.000Z" is "2026-11-08".
fn day(timestamp: &str) -> &str {
    timestamp.split('T').next().unwrap_or(timestamp)
}

impl ObservedCompute {
    /// The measured share as a sentence, or nothing when the platform sent
    /// no figure.
    fn figure(&self) -> Option<String> {
        let share = percent(self.fraction?);
        let external_only = self.scope.as_deref() == Some("external-only");
        let mut text = match (&self.window_start, &self.window_end) {
            (Some(start), Some(end)) => format!("Traffic {} to {}: ", day(start), day(end)),
            _ => "Traffic: ".to_string(),
        };
        text.push_str(&match self.measure.as_deref() {
            Some("awake") => format!("awake {} of the time", share),
            Some("busy-windows") if external_only => {
                format!("a request from outside in {} of five-minute windows", share)
            }
            Some("busy-windows") => format!("a request in {} of five-minute windows", share),
            Some(other) => format!("{} {}", other, share),
            None => share,
        });
        if let Some(source) = &self.source {
            text.push_str(&format!(" ({})", source));
        }
        text.push('.');
        if external_only {
            text.push_str(
                " Calls from the application's own services and workers are not counted, so                  the real share may be far higher.",
            );
        }
        Some(text)
    }
}

impl ServiceCompute {
    /// `auto` is a choice left to the platform, so say what it picked.
    fn summary(&self) -> String {
        if self.mode == self.runs_as {
            self.mode.clone()
        } else {
            format!("{} (runs {})", self.mode, self.runs_as)
        }
    }

    /// The reason as written when its owner wrote it; otherwise led by where
    /// it came from, so a rule is not mistaken for someone's note.
    fn reason_text(&self) -> Option<String> {
        let reason = self.reason.as_ref()?;
        Some(match self.reason_from.as_deref() {
            None | Some("owner") => reason.clone(),
            Some("platform") => format!("Platform rule: {}", reason),
            Some("deploy") => format!("Found by the last deploy: {}", reason),
            Some(other) => format!("From {}: {}", other, reason),
        })
    }

    /// What is shown under the service: the reason, the traffic figure and
    /// the recommendation, each where there is one.
    fn details(&self) -> Vec<String> {
        let observed = self.observed.as_ref();
        [
            self.reason_text(),
            observed.and_then(|o| o.figure()),
            observed.and_then(|o| o.recommendation.clone()),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compute_cmd() -> Command {
        ComputeCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        compute_cmd().debug_assert();
    }

    #[test]
    fn showing_needs_only_environment_and_region() {
        assert!(compute_cmd().try_get_matches_from(["compute"]).is_err());
        let matches = compute_cmd()
            .try_get_matches_from(["compute", "-e", "production", "-r", "us-west-2"])
            .unwrap();
        assert!(matches.get_many::<String>("service").is_none());
    }

    #[test]
    fn service_accepts_multiple_entries() {
        let matches = compute_cmd()
            .try_get_matches_from([
                "compute",
                "-e",
                "production",
                "-r",
                "us-west-2",
                "--service",
                "admin=scale-to-zero",
                "--service",
                "api=always-on",
            ])
            .unwrap();
        assert_eq!(matches.get_many::<String>("service").unwrap().count(), 2);
    }

    #[test]
    fn parse_service_modes_accepts_every_mode_by_its_public_name() {
        for mode in COMPUTE_MODES {
            let entry = format!("api={mode}");
            let parsed = parse_service_modes(&[&entry]).unwrap();
            assert_eq!(parsed.get("api").unwrap(), mode);
        }
    }

    #[test]
    fn parse_service_modes_rejects_malformed_duplicate_and_unknown_entries() {
        let missing_eq = "api".to_string();
        assert!(parse_service_modes(&[&missing_eq]).is_err());

        let empty_mode = "api=".to_string();
        assert!(parse_service_modes(&[&empty_mode]).is_err());

        let empty_service = "=always-on".to_string();
        assert!(parse_service_modes(&[&empty_service]).is_err());

        let first = "api=always-on".to_string();
        let second = "api=scale-to-zero".to_string();
        assert!(parse_service_modes(&[&first, &second]).is_err());

        // The stored value is not what an owner types.
        let stored_name = "api=microvm".to_string();
        let err = parse_service_modes(&[&stored_name])
            .unwrap_err()
            .to_string();
        assert!(err.contains("not a compute mode"), "{err}");
    }

    #[test]
    fn a_reason_goes_to_the_services_set_always_on_and_nowhere_else() {
        let a = "api=always-on".to_string();
        let b = "admin=scale-to-zero".to_string();
        let modes = parse_service_modes(&[&a, &b]).unwrap();
        let why = " called every minute ".to_string();
        let reasons = always_on_reasons(&modes, Some(&why)).unwrap();
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons.get("api").unwrap(), "called every minute");
        assert!(always_on_reasons(&modes, None).unwrap().is_empty());

        let only_sleepers = parse_service_modes(&[&b]).unwrap();
        assert!(always_on_reasons(&only_sleepers, Some(&why)).is_err());
    }

    #[test]
    fn summary_says_what_auto_came_down_to() {
        let response: ComputeResponse = serde_json::from_str(
            r#"{
                "applicationId": "app-1",
                "environment": "production",
                "region": "us-west-2",
                "services": [
                    {"service": "admin", "mode": "auto", "runsAs": "scale-to-zero"},
                    {"service": "api", "mode": "always-on", "runsAs": "always-on"},
                    {"service": "ledger", "mode": "auto", "runsAs": "always-on",
                     "reason": "MICROVM_NOT_ALLOWED_FOR_COMPLIANCE: scoped to HIPAA."}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(response.services[0].summary(), "auto (runs scale-to-zero)");
        assert_eq!(response.services[1].summary(), "always-on");
        assert_eq!(response.services[2].summary(), "auto (runs always-on)");
        assert!(response.services[2].reason.is_some());
        assert!(response.message.is_none());
    }

    #[test]
    fn an_undeployed_environment_parses_with_only_a_message() {
        let response: ComputeResponse = serde_json::from_str(
            r#"{"applicationId": "app-1", "environment": "staging", "region": "us-east-2",
                "services": [], "message": "No service has been deployed here yet."}"#,
        )
        .unwrap();
        assert!(response.services.is_empty());
        assert!(response.message.is_some());
    }

    #[test]
    fn a_reason_says_where_it_came_from_unless_its_owner_wrote_it() {
        let response: ComputeResponse = serde_json::from_str(
            r#"{"environment": "production", "region": "us-west-2", "services": [
                {"service": "api", "mode": "always-on", "runsAs": "always-on",
                 "reason": "called every minute by the publisher", "reasonFrom": "owner"},
                {"service": "ledger", "mode": "auto", "runsAs": "always-on",
                 "reason": "The application declares HIPAA.", "reasonFrom": "platform"},
                {"service": "chat", "mode": "auto", "runsAs": "always-on",
                 "reason": "It serves WebSockets.", "reasonFrom": "deploy"},
                {"service": "old", "mode": "always-on", "runsAs": "always-on",
                 "reason": "not reviewed yet"}
            ]}"#,
        )
        .unwrap();
        let texts: Vec<String> = response
            .services
            .iter()
            .map(|service| service.reason_text().unwrap())
            .collect();
        assert_eq!(
            texts,
            [
                "called every minute by the publisher",
                "Platform rule: The application declares HIPAA.",
                "Found by the last deploy: It serves WebSockets.",
                "not reviewed yet",
            ]
        );
    }

    fn observed_services() -> Vec<ServiceCompute> {
        let response: ComputeResponse = serde_json::from_str(
            r#"{"environment": "production", "region": "us-west-2", "services": [
                {"service": "admin", "mode": "auto", "runsAs": "scale-to-zero"},
                {"service": "billing", "mode": "always-on", "runsAs": "always-on",
                 "reason": "not reviewed yet", "reasonFrom": "owner",
                 "observed": {"measure": "busy-windows", "fraction": 0.004,
                    "windowStart": "2026-11-08T00:00:00.000Z",
                    "windowEnd": "2026-11-15T00:00:00.000Z",
                    "source": "container requests", "scope": "all",
                    "advice": "move-to-scale-to-zero",
                    "recommendation": "Move it to scale to zero (set it to auto), after checking it does no work between requests such as a timer. A request arrived in under 1% of five-minute windows. On scale to zero it would be awake at most 1% of the time and cost about $0.63 a month; always on costs $2.50 to $6.00 (shared pool list price). The two cost the same at 9% to 25% awake."}},
                {"service": "leads", "mode": "auto", "runsAs": "scale-to-zero",
                 "observed": {"measure": "awake", "fraction": 0.4,
                    "windowStart": "2026-11-08T00:00:00.000Z",
                    "windowEnd": "2026-11-15T00:00:00.000Z",
                    "source": "wake router requests", "scope": "all",
                    "advice": "make-always-on",
                    "recommendation": "Make it always on. Awake 40% of the time, scale to zero costs about $9.47 a month; always on costs $2.50 to $6.00 (shared pool list price). The two cost the same at 9% to 25% awake."}},
                {"service": "search", "mode": "always-on", "runsAs": "always-on",
                 "observed": {"measure": "busy-windows", "fraction": 0.03,
                    "windowStart": "2026-11-08T00:00:00.000Z",
                    "windowEnd": "2026-11-15T00:00:00.000Z",
                    "source": "load balancer requests", "scope": "external-only",
                    "advice": "none",
                    "recommendation": "No advice. From outside, a request arrived in 3% of five-minute windows, but that is external traffic only."}}
            ]}"#,
        )
        .unwrap();
        response.services
    }

    #[test]
    fn observed_traffic_reads_as_a_share_over_a_window_with_its_source() {
        let services = observed_services();
        assert!(services[0].details().is_empty());
        assert_eq!(
            services[1].observed.as_ref().unwrap().figure().unwrap(),
            "Traffic 2026-11-08 to 2026-11-15: a request in under 1% of five-minute windows \
             (container requests)."
        );
        assert_eq!(
            services[2].observed.as_ref().unwrap().figure().unwrap(),
            "Traffic 2026-11-08 to 2026-11-15: awake 40% of the time (wake router requests)."
        );
        // The recommendation follows the figure, in the platform's words.
        assert!(services[2].details()[1].starts_with("Make it always on."));
    }

    #[test]
    fn external_only_traffic_says_internal_calls_are_not_counted() {
        let services = observed_services();
        let figure = services[3].observed.as_ref().unwrap().figure().unwrap();
        assert!(
            figure.starts_with(
                "Traffic 2026-11-08 to 2026-11-15: a request from outside in 3% of five-minute \
                 windows (load balancer requests)."
            ),
            "{figure}"
        );
        assert!(
            figure.contains("own services and workers are not counted"),
            "{figure}"
        );
    }

    #[test]
    fn the_listing_stays_within_100_columns_and_indents_details_under_the_mode() {
        let lines = service_lines(&observed_services());
        assert!(
            lines.iter().all(|line| line.chars().count() <= 100),
            "{lines:#?}"
        );
        assert_eq!(lines[0], "    admin    auto (runs scale-to-zero)");
        assert_eq!(lines[1], "    billing  always-on");
        assert_eq!(lines[2], "               not reviewed yet");
        // Nothing is lost in the wrapping.
        let billing = &observed_services()[1];
        let printed = lines[2..]
            .iter()
            .take_while(|line| line.starts_with("     "))
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(printed, billing.details().join(" "));
    }

    #[test]
    fn wrap_breaks_at_spaces_and_leaves_a_long_word_whole() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("a verylongword b", 4), ["a", "verylongword", "b"]);
        assert!(wrap("", 10).is_empty());
    }

    #[test]
    fn an_older_platform_without_origin_or_traffic_still_parses() {
        let response: ComputeResponse = serde_json::from_str(
            r#"{"environment": "production", "region": "us-west-2", "services": [
                {"service": "api", "mode": "always-on", "runsAs": "always-on",
                 "reason": "called every minute"},
                {"service": "odd", "mode": "auto", "runsAs": "scale-to-zero",
                 "observed": {"recommendation": "Leave it."}}
            ]}"#,
        )
        .unwrap();
        assert!(response.services[0].reason_from.is_none());
        assert!(response.services[0].observed.is_none());
        assert_eq!(response.services[0].details(), ["called every minute"]);
        // A figure with no fraction is not invented.
        assert_eq!(response.services[1].details(), ["Leave it."]);
    }
}
