use std::io::Write;

use anyhow::{Context, Result};
use clap::{ArgMatches, Command};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{command::command, validate::require_auth},
};

#[derive(Debug)]
pub(super) struct WhoAmICommand;

impl WhoAmICommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

#[derive(Deserialize)]
struct MeResponse {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default, rename = "organizationName")]
    organization_name: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    roles: Option<Vec<String>>,
    #[serde(default, rename = "trialActive")]
    trial_active: Option<bool>,
    #[serde(default, rename = "trialDaysRemaining")]
    trial_days_remaining: Option<f64>,
    #[serde(default)]
    subscription: Option<SubscriptionInfo>,
}

#[derive(Deserialize)]
struct SubscriptionInfo {
    #[serde(default, rename = "planName")]
    plan_name: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponse {
    response: MeResponse,
}

impl CliCommand for WhoAmICommand {
    fn command(&self) -> Command {
        command("whoami", "Get the current user")
    }

    fn handler(&self, _matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let token = require_auth()?;

        let api_url = get_platform_management_api_url();
        let client = reqwest::blocking::Client::new();
        let response = client
            .get(format!("{}/user-profile/me", api_url))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .with_context(|| "Failed to reach platform API")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch user info (HTTP {})",
                response.status()
            );
        }

        let api_response: ApiResponse = response
            .json()
            .with_context(|| "Failed to parse user info response")?;
        let me = api_response.response;

        writeln!(stdout)?;

        // Name
        if let Some(name) = &me.name {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Name:         ")?;
            stdout.reset()?;
            writeln!(stdout, "{}", name)?;
        }

        // Email
        if let Some(email) = &me.email {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Email:        ")?;
            stdout.reset()?;
            writeln!(stdout, "{}", email)?;
        }

        // Organization
        if let Some(org) = &me.organization_name {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Organization: ")?;
            stdout.reset()?;
            writeln!(stdout, "{}", org)?;
        }

        // Role
        if let Some(role) = &me.role {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Role:         ")?;
            stdout.reset()?;
            writeln!(stdout, "{}", role)?;
        } else if let Some(roles) = &me.roles {
            if !roles.is_empty() {
                stdout.set_color(ColorSpec::new().set_bold(true))?;
                write!(stdout, "  Roles:        ")?;
                stdout.reset()?;
                writeln!(stdout, "{}", roles.join(", "))?;
            }
        }

        // Subscription / Trial
        if let Some(sub) = &me.subscription {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Plan:         ")?;
            stdout.reset()?;
            let plan = sub.plan_name.as_deref().unwrap_or("Unknown");
            let status = sub.status.as_deref().unwrap_or("unknown");
            writeln!(stdout, "{} ({})", plan, status)?;
        } else if me.trial_active == Some(true) {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  Plan:         ")?;
            stdout.reset()?;
            if let Some(days) = me.trial_days_remaining {
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
                writeln!(stdout, "Trial ({} days remaining)", days as i64)?;
            } else {
                stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
                writeln!(stdout, "Trial")?;
            }
            stdout.reset()?;
        }

        writeln!(stdout)?;

        Ok(())
    }
}
