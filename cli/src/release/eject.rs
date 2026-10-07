use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
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

/// Generates the exact Pulumi TypeScript that ForkLaunch would deploy for a
/// release — for customers leaving managed hosting to self-host via Pulumi.
/// Distinct from the top-level `fl eject`, which strips framework abstractions
/// from your own local source; this one asks the platform to generate
/// infrastructure-as-code from a release manifest.
#[derive(Debug)]
pub(crate) struct EjectCommand;

impl EjectCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for EjectCommand {
    fn command(&self) -> Command {
        command(
            "eject",
            "Generate the Pulumi infrastructure-as-code a release would deploy, for self-hosting outside ForkLaunch",
        )
        .arg(
            Arg::new("release_version")
                .long("release")
                .required(true)
                .help("Release version to generate infra for"),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment (e.g. production)"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("AWS region"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .conflicts_with("json")
                .help("Write the generated Pulumi code to this file instead of stdout"),
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
                .help("Output the full raw response as JSON instead of formatted text (cannot be combined with --output — --json always prints to stdout)")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let release_version = matches
            .get_one::<String>("release_version")
            .context("--release is required")?;
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let output = matches.get_one::<String>("output");
        let json_output = matches.get_flag("json");

        let body = serde_json::json!({
            "applicationId": application_id,
            "releaseVersion": release_version,
            "environment": environment,
            "region": region,
        });

        let url = format!("{}/eject/generate", get_platform_management_api_url());
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!(
                "Failed to generate eject infrastructure: {}",
                response.text().unwrap_or_default()
            );
        }

        let result: EjectResponse = response
            .json()
            .with_context(|| "Failed to parse eject response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        if let Some(path) = output {
            std::fs::write(path, &result.pulumi_code)
                .with_context(|| format!("Failed to write Pulumi code to {}", path))?;
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Generated")?;
        stdout.reset()?;
        writeln!(stdout, "  infrastructure for release {}", result.release_info.version)?;

        if let Some(path) = output {
            writeln!(stdout, "  Pulumi code written to {}", path)?;
        }

        if !result.instructions.prerequisites.is_empty() {
            writeln!(stdout)?;
            writeln!(stdout, "  Prerequisites:")?;
            for p in &result.instructions.prerequisites {
                writeln!(stdout, "    - {}", p)?;
            }
        }
        if !result.instructions.steps.is_empty() {
            writeln!(stdout)?;
            writeln!(stdout, "  Steps:")?;
            for (i, s) in result.instructions.steps.iter().enumerate() {
                writeln!(stdout, "    {}. {}", i + 1, s)?;
            }
        }
        if !result.instructions.notes.is_empty() {
            writeln!(stdout)?;
            writeln!(stdout, "  Notes:")?;
            for n in &result.instructions.notes {
                writeln!(stdout, "    - {}", n)?;
            }
        }

        if output.is_none() {
            writeln!(stdout)?;
            writeln!(stdout, "  --- Pulumi code ---")?;
            writeln!(stdout, "{}", result.pulumi_code)?;
        }
        writeln!(stdout)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseInfo {
    #[serde(default)]
    id: Option<String>,
    version: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Instructions {
    #[serde(default)]
    prerequisites: Vec<String>,
    #[serde(default)]
    steps: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EjectResponse {
    pulumi_code: String,
    release_info: ReleaseInfo,
    #[serde(default)]
    instructions: Instructions,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eject_cmd() -> Command {
        EjectCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        eject_cmd().debug_assert();
    }

    #[test]
    fn requires_release_environment_and_region() {
        assert!(eject_cmd().try_get_matches_from(["eject"]).is_err());
        assert!(
            eject_cmd()
                .try_get_matches_from([
                    "eject", "--release", "1.0.0", "-e", "production", "-r", "us-east-1"
                ])
                .is_ok()
        );
    }

    #[test]
    fn output_and_json_are_mutually_exclusive() {
        let base = [
            "eject", "--release", "1.0.0", "-e", "production", "-r", "us-east-1",
        ];
        assert!(
            eject_cmd()
                .try_get_matches_from(base.iter().chain(["--output", "out.ts"].iter()))
                .is_ok()
        );
        assert!(
            eject_cmd()
                .try_get_matches_from(base.iter().chain(["--json"].iter()))
                .is_ok()
        );
        assert!(
            eject_cmd()
                .try_get_matches_from(
                    base.iter().chain(["--output", "out.ts", "--json"].iter())
                )
                .is_err()
        );
    }

    #[test]
    fn eject_response_deserializes() {
        let json = r#"{
            "pulumiCode": "const x = 1;",
            "releaseInfo": {"id": "rel-1", "version": "1.0.0"},
            "instructions": {"prerequisites": ["aws cli"], "steps": ["step 1"], "notes": []}
        }"#;
        let response: EjectResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.release_info.version, "1.0.0");
        assert_eq!(response.instructions.steps.len(), 1);
    }
}
