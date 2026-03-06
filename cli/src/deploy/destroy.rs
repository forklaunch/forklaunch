use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url, get_platform_ui_url},
    core::{
        command::command,
        http_client,
        validate::{require_active_account, require_integration, require_manifest, resolve_auth},
    },
    deploy::utils::stream_deployment_status,
};

#[derive(Debug, Serialize)]
struct DestroyDeploymentRequest {
    mode: String,
}

#[derive(Debug, Deserialize)]
struct DestroyDeploymentResponse {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    status: String,
}

#[derive(Debug)]
pub(crate) struct DestroyCommand;

impl DestroyCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for DestroyCommand {
    fn command(&self) -> Command {
        command("destroy", "Destroy application infrastructure")
            .arg(
                Arg::new("environment")
                    .long("environment")
                    .short('e')
                    .required(true)
                    .help("Environment name (e.g., staging, production)"),
            )
            .arg(
                Arg::new("region")
                    .long("region")
                    .required(true)
                    .help("AWS region (e.g., us-east-1)"),
            )
            .arg(
                Arg::new("mode")
                    .long("mode")
                    .help("Destroy mode: 'all' (default) or 'preserve-data'")
                    .value_parser(["all", "preserve-data"]),
            )
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root (optional)"),
            )
            .arg(
                Arg::new("no-wait")
                    .long("no-wait")
                    .action(clap::ArgAction::SetTrue)
                    .help("Don't wait for destruction to complete"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let auth_mode = resolve_auth()?;
        require_active_account(&auth_mode)?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let environment = matches
            .get_one::<String>("environment")
            .ok_or_else(|| anyhow::anyhow!("Environment is required"))?
            .to_lowercase();

        let region = matches
            .get_one::<String>("region")
            .ok_or_else(|| anyhow::anyhow!("Region is required"))?;

        let mode = matches
            .get_one::<String>("mode")
            .cloned()
            .unwrap_or_else(|| "all".to_string());

        let wait = !matches.get_flag("no-wait");

        log_header!(stdout, Color::Red, "DESTROYING INFRASTRUCTURE: {} ({}) [{}]",
            environment, region, mode
        );
        writeln!(stdout)?;

        // Confirmation prompt?
        // The implementation plan didn't explicitly ask for one in the logic section, but frontend has it.
        // For CLI, explicit destroy command is usually enough, but let's add a warning if not "preserve-data".
        // Actually, let's trust the user knows what they are doing if they run `destroy`.

        // Removed the old request_body and client setup
        // let request_body = DestroyDeploymentRequest { mode: mode.clone() };
        // let url = format!(
        //     "{}/applications/{}/environments/{}/regions/{}/destroy",
        //     get_platform_management_api_url(),
        //     application_id,
        //     environment,
        //     region
        // );
        // let client = Client::new();

        log_progress!(stdout, "Triggering destruction...");

        let request_body = DestroyDeploymentRequest { mode: mode.clone() };

        let url = format!(
            "{}/applications/{}/environments/{}/regions/{}/destroy",
            get_platform_management_api_url(),
            application_id,
            environment,
            region
        );

        let response =
            http_client::post_with_auth(&auth_mode, &url, serde_json::to_value(&request_body)?)
                .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();

        if status.is_success() {
            let response_text = response.text().with_context(|| "Failed to read response")?;
            let deployment: DestroyDeploymentResponse = serde_json::from_str(&response_text)
                .with_context(|| format!("Failed to parse destroy response: {}", response_text))?;

            log_ok_suffix!(stdout);
            log_info!(stdout, "Deployment ID: {}", deployment.id);

            if wait {
                writeln!(stdout)?;
                stream_deployment_status(
                    &auth_mode,
                    &deployment.id,
                    &mut stdout,
                )?;
            } else {
                writeln!(stdout)?;
                log_info!(stdout, "Destruction started. Check status at:");
                writeln!(
                    stdout,
                    "  {}/apps/{}/deployments/{}",
                    get_platform_ui_url(), application_id, deployment.id
                )?;
            }
        } else if status.as_u16() == 409 {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());

            log_error_suffix!(stdout);

            anyhow::bail!(
                "Deployment conflict: {}. Wait for the current deployment to complete or cancel it first.",
                error_text
            );
        } else if status.as_u16() == 403 {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());

            log_error_suffix!(stdout);

            anyhow::bail!("{}", error_text);
        } else {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());

            log_error_suffix!(stdout);

            anyhow::bail!(
                "Failed to destroy infrastructure: {} (Status: {})",
                error_text,
                status
            );
        }

        Ok(())
    }
}
