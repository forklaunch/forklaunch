use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct ServicesCommand;

impl ServicesCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ServicesCommand {
    fn command(&self) -> Command {
        command(
            "services",
            "List services (and their associated workers) for this application",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .help("Filter to an environment"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .help("Filter to a region"),
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Path to application root (optional)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches.get_one::<String>("environment");
        let region = matches.get_one::<String>("region");
        let json_output = matches.get_flag("json");

        let mut url = format!(
            "{}/applications/{}/services",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id)
        );
        let mut query_parts = Vec::new();
        if let Some(e) = environment {
            query_parts.push(format!("environment={}", urlencoding::encode(e)));
        }
        if let Some(r) = region {
            query_parts.push(format!("region={}", urlencoding::encode(r)));
        }
        if !query_parts.is_empty() {
            url.push('?');
            url.push_str(&query_parts.join("&"));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list services: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: ServicesResponse = response
            .json()
            .with_context(|| "Failed to parse services response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        print_services(&result.services)
    }
}

fn print_services(services: &[ServiceSummary]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if services.is_empty() {
        writeln!(stdout, "No services found.")?;
        return Ok(());
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<36}  {:<20}  {:<14}  {:<10}  {}",
        "ID", "NAME", "STATUS", "TYPE", "VERSION"
    )?;
    stdout.reset()?;

    for s in services {
        writeln!(
            stdout,
            "  {:<36}  {:<20}  {:<14}  {:<10}  {}",
            s.id,
            s.name,
            s.status,
            s.r#type.as_deref().unwrap_or("-"),
            s.version.as_deref().unwrap_or("-")
        )?;
    }
    writeln!(stdout)?;

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceSummary {
    id: String,
    name: String,
    status: String,
    #[serde(default, rename = "type")]
    r#type: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServicesResponse {
    services: Vec<ServiceSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn services_cmd() -> Command {
        ServicesCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        services_cmd().debug_assert();
    }

    #[test]
    fn has_no_required_args() {
        assert!(services_cmd().try_get_matches_from(["services"]).is_ok());
    }

    #[test]
    fn services_response_deserializes() {
        let json = r#"{"services": [{"id": "s1", "name": "iam", "status": "RUNNING", "type": "service", "version": "1.0.0"}]}"#;
        let response: ServicesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.services.len(), 1);
        assert_eq!(response.services[0].name, "iam");
    }
}
