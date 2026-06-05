use std::{fs::write, io::Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
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

#[derive(Debug, Deserialize)]
struct MeIdentity {
    id: String,
    #[serde(rename = "organizationId")]
    organization_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreatedApplication {
    id: String,
    name: String,
    #[serde(rename = "organizationId")]
    organization_id: String,
}

#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command(
            "create",
            "Create a platform application and integrate the local project in one step",
        )
        .arg(
            Arg::new("name")
                .long("name")
                .short('n')
                .help("Application name (defaults to the local app_name from manifest.toml)"),
        )
        .arg(
            Arg::new("description")
                .long("description")
                .short('D')
                .help("Application description"),
        )
        .arg(
            Arg::new("no_integrate")
                .long("no-integrate")
                .action(ArgAction::SetTrue)
                .help("Only create the platform application; skip linking the local project"),
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
        let skip_integrate = matches.get_flag("no_integrate");

        if !skip_integrate {
            if let Some(existing_id) = &manifest.platform_application_id {
                bail!(
                    "This application is already integrated with platform application ID: {}. Use --no-integrate to create another platform app without linking, or remove platform_application_id from .forklaunch/manifest.toml first.",
                    existing_id
                );
            }
        }

        let name = matches
            .get_one::<String>("name")
            .cloned()
            .unwrap_or_else(|| manifest.app_name.clone());
        let description = matches
            .get_one::<String>("description")
            .cloned()
            .unwrap_or_else(|| manifest.app_description.clone());

        let api_url = get_platform_management_api_url();
        let client = Client::new();

        // Resolve the caller's identity — createApplication requires the
        // owning userId + organizationId, which live server-side on /me.
        let me: MeIdentity = client
            .get(format!("{}/user-profile/me", api_url))
            .bearer_auth(&token)
            .send()
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?
            .error_for_status()
            .with_context(|| "Failed to resolve current user (are you logged in?)")?
            .json()
            .with_context(|| "Failed to parse /user-profile/me response")?;

        log_info!(stdout, "Creating platform application '{}'...", name);
        let create_response = client
            .post(format!("{}/applications/", api_url))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "name": name,
                "description": description,
                "userId": me.id,
                "organizationId": me.organization_id,
                "isDeleted": false
            }))
            .send()
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = create_response.status();
        if !status.is_success() {
            bail!(
                "Failed to create application '{}': {} ({})",
                name,
                create_response.text().unwrap_or_default(),
                status
            );
        }

        let app: CreatedApplication = create_response
            .json()
            .with_context(|| "Failed to parse created application response")?;
        log_ok!(stdout, "Created platform application: {} ({})", app.name, app.id);

        if skip_integrate {
            log_info!(
                stdout,
                "Skipping integration (--no-integrate). Link later with: forklaunch integrate --app {}",
                app.id
            );
            return Ok(());
        }

        // Same linking flow as `forklaunch integrate --app <id>`.
        log_info!(stdout, "Integrating local project...");
        let integrate_response = client
            .post(format!("{}/applications/{}/integrate", api_url, app.id))
            .bearer_auth(&token)
            .send()
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !integrate_response.status().is_success() {
            bail!(
                "Application {} was created but integration failed ({}). Link manually with: forklaunch integrate --app {}",
                app.id,
                integrate_response.status(),
                app.id
            );
        }

        let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
        let manifest_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest at {:?}", manifest_path))?;
        let mut manifest: ApplicationManifestData =
            toml::from_str(&manifest_content).with_context(|| "Failed to parse manifest.toml")?;
        manifest.platform_application_id = Some(app.id.clone());
        manifest.platform_organization_id = Some(app.organization_id.clone());
        let updated_manifest =
            to_string_pretty(&manifest).with_context(|| "Failed to serialize updated manifest")?;
        write(&manifest_path, updated_manifest)
            .with_context(|| format!("Failed to write manifest at {:?}", manifest_path))?;

        log_header!(stdout, Color::Green, "\nApplication created and integrated!");
        log_info!(stdout, "Platform App ID: {}", app.id);
        log_info!(stdout, "\nYou can now use:");
        writeln!(stdout, "  forklaunch release create --version <version>")?;
        writeln!(
            stdout,
            "  forklaunch deploy create --release <version> --environment <env> --region <region>"
        )?;

        Ok(())
    }
}
