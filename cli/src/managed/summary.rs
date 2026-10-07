use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{command::command, hmac::AuthMode, http_client::get_with_auth},
};

/// `forklaunch managed summary`
///
/// Two questions this answers that nothing else does: which instances are
/// running and whether sign-in would actually work for them, and the exact
/// callback URL to register with the OAuth provider. Both come from one
/// control-plane call, so the output cannot show a half-answer.
#[derive(Debug)]
pub(super) struct SummaryCommand;

impl SummaryCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for SummaryCommand {
    fn command(&self) -> Command {
        command(
            "summary",
            "Managed instances, their sign-in eligibility, and the relay contract per template",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let json_output = matches.get_flag("json");
        let summary = fetch_summary()?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            print_summary(&summary)?;
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedInstanceSummary {
    id: String,
    template_slug: String,
    host: String,
    region: String,
    state: String,
    relay_eligible: bool,
    #[serde(default)]
    current_version_semver: Option<String>,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayConfigSummary {
    template_slug: String,
    callback_url: String,
    state_format: String,
    host_pattern: String,
    eligible_states: Vec<String>,
    state_ttl_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Counts {
    total: u64,
    relay_eligible: u64,
    failed: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedModeSummary {
    available: bool,
    #[serde(default)]
    unavailable_reason: Option<String>,
    instances: Vec<ManagedInstanceSummary>,
    relay_configs: Vec<RelayConfigSummary>,
    counts: Counts,
}

fn fetch_summary() -> Result<ManagedModeSummary> {
    let url = format!("{}/managed-mode/summary", get_platform_management_api_url());
    let auth_mode = AuthMode::detect();
    let response =
        get_with_auth(&auth_mode, &url).with_context(|| "Failed to reach the control plane")?;

    if !response.status().is_success() {
        let http_status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Control plane returned {} — {}", http_status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse managed mode summary")
}

fn print_summary(summary: &ManagedModeSummary) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    if !summary.available {
        // Say why, rather than printing an empty list that reads as
        // "you have no instances".
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
        writeln!(stdout, "Managed mode is off")?;
        stdout.reset()?;
        if let Some(reason) = &summary.unavailable_reason {
            writeln!(stdout, "  {}", reason)?;
        }
        return Ok(());
    }

    stdout.set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Cyan)))?;
    writeln!(stdout, "Managed apps")?;
    stdout.reset()?;
    writeln!(
        stdout,
        "  {} instance(s) · {} sign-in eligible · {} reporting errors",
        summary.counts.total, summary.counts.relay_eligible, summary.counts.failed
    )?;
    writeln!(stdout)?;

    if !summary.relay_configs.is_empty() {
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(stdout, "  OAuth relay")?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  Register this as the redirect URI — one per template, shared by all its instances."
        )?;
        for cfg in &summary.relay_configs {
            writeln!(stdout)?;
            writeln!(stdout, "    {}", cfg.template_slug)?;
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            writeln!(stdout, "      {}", cfg.callback_url)?;
            stdout.reset()?;
            writeln!(stdout, "      state    {}", cfg.state_format)?;
            writeln!(
                stdout,
                "      expires  {} min",
                cfg.state_ttl_seconds / 60
            )?;
            writeln!(
                stdout,
                "      routed   while {}",
                cfg.eligible_states.join(", ")
            )?;
        }
        writeln!(stdout)?;
    }

    if summary.instances.is_empty() {
        writeln!(stdout, "  No instances yet.")?;
        return Ok(());
    }

    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stdout, "  Instances")?;
    stdout.reset()?;

    for instance in &summary.instances {
        // Sign-in eligibility is the same check the relay applies, so an
        // ineligible instance will have its callback refused.
        let (marker, color) = if instance.relay_eligible {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Yellow)
        };
        stdout.set_color(ColorSpec::new().set_fg(Some(color)))?;
        write!(stdout, "    {} ", marker)?;
        stdout.reset()?;
        write!(stdout, "{}", instance.host)?;
        writeln!(
            stdout,
            "  [{}]{}",
            instance.state,
            instance
                .current_version_semver
                .as_ref()
                .map(|v| format!(" v{}", v))
                .unwrap_or_default()
        )?;
        if let Some(err) = &instance.last_error {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
            writeln!(stdout, "        {}", err)?;
            stdout.reset()?;
        }
    }

    Ok(())
}
