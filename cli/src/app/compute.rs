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
//! A new service starts on `auto`: the platform runs it scale to zero when it
//! can run that way, and always on when it cannot (it serves WebSockets, or
//! the application declares a compliance framework, for example). A service
//! that already existed keeps running the way it does until someone changes
//! it here.
//!
//! With no `--service`, this command shows each service's mode. With one or
//! more, it records the change, which takes effect on the next deployment.
//! Workers are not listed: nothing calls a worker, so nothing could wake it.

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
            http_client::patch(
                &format!("{}/applications/{}/compute", api_url, application_id),
                serde_json::json!({
                    "environment": environment,
                    "region": region,
                    "services": parse_service_modes(&changes)?,
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

fn render(stdout: &mut StandardStream, compute: &ComputeResponse) -> Result<()> {
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    write!(
        stdout,
        "  Compute: {} ({})",
        compute.environment, compute.region
    )?;
    stdout.reset()?;
    writeln!(stdout)?;
    let width = compute
        .services
        .iter()
        .map(|service| service.service.len())
        .max()
        .unwrap_or(0);
    for service in &compute.services {
        writeln!(
            stdout,
            "    {:<width$}  {}",
            service.service,
            service.summary(),
            width = width
        )?;
        if let Some(reason) = &service.reason {
            writeln!(stdout, "    {:<width$}    {}", "", reason, width = width)?;
        }
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
    /// What that comes down to on a deployment made now.
    runs_as: String,
    /// Why auto comes down to always-on, when it does.
    #[serde(default)]
    reason: Option<String>,
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
}
