use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(crate) struct CheckCommand;

impl CheckCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CheckCommand {
    fn command(&self) -> Command {
        command("check", "Check the current organization for hosting drift").arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let json_output = matches.get_flag("json");

        let org_id = fetch_organization_id()?;

        let url = format!(
            "{}/hosting/organizations/{}/hosting-drift",
            get_platform_management_api_url(),
            urlencoding::encode(&org_id)
        );
        let response = http_client::get(&url).with_context(|| "Failed to reach platform API")?;
        if !response.status().is_success() {
            bail!(
                "Failed to check hosting drift: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: HostingDriftResponse = response
            .json()
            .with_context(|| "Failed to parse hosting drift response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        print_rows(&result.drift, result.plan_name.as_deref())
    }
}

/// The hosting-drift endpoint requires the caller's own organization id in the
/// URL (the backend rejects any other org unless the caller has the SYSTEM
/// role) — fetch it from the user profile rather than asking for it as a flag.
fn fetch_organization_id() -> Result<String> {
    let url = format!("{}/user-profile/me", get_platform_management_api_url());
    let response = http_client::get(&url).with_context(|| "Failed to reach platform API")?;
    if !response.status().is_success() {
        bail!(
            "Failed to resolve current organization: {}",
            response.text().unwrap_or_default()
        );
    }
    let profile: UserProfile = response
        .json()
        .with_context(|| "Failed to parse user profile response")?;
    profile
        .organization_id
        .context("Current user has no organization — join or create one first")
}

fn print_rows(rows: &[DriftRow], plan_name: Option<&str>) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if rows.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        writeln!(stdout, "No hosting drift detected.")?;
        stdout.reset()?;
        return Ok(());
    }

    if let Some(plan) = plan_name {
        writeln!(stdout, "  Plan: {}", plan)?;
    }
    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<20}  {:<12}  {:<20}  {:<20}  {}",
        "COMPONENT", "TYPE", "APPLICATION", "CURRENT TIER", "REASON"
    )?;
    stdout.reset()?;

    for row in rows {
        writeln!(
            stdout,
            "  {:<20}  {:<12}  {:<20}  {:<20}  {}",
            row.component_name, row.component_type, row.application_name, row.current_hosting_type, row.reason
        )?;
    }
    writeln!(stdout)?;
    writeln!(stdout, "  {} drifted component(s).", rows.len())?;
    writeln!(stdout)?;

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    #[serde(default)]
    organization_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostingDriftResponse {
    #[serde(default)]
    org_id: Option<String>,
    #[serde(default)]
    plan_name: Option<String>,
    drift: Vec<DriftRow>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriftRow {
    #[serde(default)]
    infrastructure_id: Option<String>,
    component_type: String,
    #[serde(default)]
    component_id: Option<String>,
    component_name: String,
    #[serde(default)]
    application_id: Option<String>,
    application_name: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    region: Option<String>,
    current_hosting_type: String,
    reason: String,
    #[serde(default)]
    suggested_tier: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_cmd() -> Command {
        CheckCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        check_cmd().debug_assert();
    }

    #[test]
    fn user_profile_deserializes_org_id() {
        let json = r#"{"id": "u1", "organizationId": "org-1"}"#;
        let profile: UserProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.organization_id.as_deref(), Some("org-1"));
    }

    #[test]
    fn drift_row_deserializes() {
        let json = r#"{
            "infrastructureId": "infra-1",
            "componentType": "service",
            "componentId": "svc-1",
            "componentName": "iam",
            "applicationId": "app-1",
            "applicationName": "myapp",
            "environment": "production",
            "region": "us-east-1",
            "currentHostingType": "shared-ec2",
            "reason": "no longer on the allowlist",
            "suggestedTier": "dedicated-ecs"
        }"#;
        let row: DriftRow = serde_json::from_str(json).unwrap();
        assert_eq!(row.component_name, "iam");
        assert_eq!(row.current_hosting_type, "shared-ec2");
    }

    #[test]
    fn hosting_drift_response_deserializes_with_empty_drift() {
        let json = r#"{"orgId": "org-1", "planName": "pro", "drift": []}"#;
        let response: HostingDriftResponse = serde_json::from_str(json).unwrap();
        assert!(response.drift.is_empty());
        assert_eq!(response.plan_name.as_deref(), Some("pro"));
    }
}
