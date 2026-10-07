use std::{fs::write, io::Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};
use toml::to_string_pretty;

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        manifest::application::ApplicationManifestData,
        validate::{require_auth, require_manifest},
    },
};

#[derive(Debug, Serialize, Deserialize)]
struct ApplicationResponse {
    id: String,
    name: String,
    description: Option<String>,
    #[serde(rename = "organizationId")]
    organization_id: String,
}

#[derive(Debug)]
pub(crate) struct IntegrateCommand;

impl IntegrateCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for IntegrateCommand {
    fn command(&self) -> Command {
        command(
            "integrate",
            "Integrate local application with ForkLaunch platform",
        )
        .arg(
            Arg::new("app")
                .long("app")
                .short('a')
                .required(true)
                .help("Platform application ID to link to"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let token = require_auth()?;
        let (app_root, manifest) = require_manifest(matches)?;

        if let Some(existing_id) = &manifest.platform_application_id {
            bail!(
                "This application is already integrated with platform application ID: {}. To re-integrate, remove the platform_application_id from .forklaunch/manifest.toml first.",
                existing_id
            );
        }

        let application_id = matches
            .get_one::<String>("app")
            .ok_or_else(|| anyhow::anyhow!("Application ID is required"))?;

        let manifest_path = app_root.join(".forklaunch").join("manifest.toml");

        // Integrate with platform application
        log_info!(stdout, "Integrating with platform application...");

        let url = format!(
            "{}/applications/{}/integrate",
            get_platform_management_api_url(),
            application_id
        );
        let client = Client::new();
        let response = client
            .post(&url)
            .bearer_auth(&token)
            .send()
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if status == reqwest::StatusCode::CONFLICT {
            bail!(
                "This platform application is already integrated with another local app. Each platform application can only be linked to one local app at a time."
            );
        }

        if !status.is_success() {
            bail!(
                "Failed to integrate application: {} (Status: {})",
                application_id,
                status
            );
        }

        let app_data: ApplicationResponse = response
            .json()
            .with_context(|| "Failed to parse application response")?;

        log_ok!(stdout, "Integrated with application: {}", app_data.name);

        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest at {:?}", manifest_path))?;

        let mut manifest: ApplicationManifestData =
            toml::from_str(&manifest_content).with_context(|| "Failed to parse manifest.toml")?;

        manifest.platform_application_id = Some(application_id.clone());
        manifest.platform_organization_id = Some(app_data.organization_id.clone());

        let updated_manifest =
            to_string_pretty(&manifest).with_context(|| "Failed to serialize updated manifest")?;

        write(&manifest_path, updated_manifest)
            .with_context(|| format!("Failed to write manifest at {:?}", manifest_path))?;

        log_header!(stdout, Color::Green, "\nApplication integrated successfully!");

        log_info!(stdout, "Platform App ID: {}", application_id);
        log_info!(stdout, "Application Name: {}", app_data.name);
        writeln!(
            stdout,
            "[INFO] Organization ID: {}",
            app_data.organization_id
        )?;

        log_info!(stdout, "\nYou can now use:");
        writeln!(stdout, "  forklaunch release create --version <version>")?;
        writeln!(
            stdout,
            "  forklaunch deploy create --release <version> --environment <env> --region <region>"
        )?;

        Ok(())
    }
}
