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

pub(crate) const INSTANCE_SIZES: &[&str] = &[
    "nano", "micro", "small", "medium", "large", "xlarge", "2xlarge",
];
pub(crate) const APPLY_MODES: &[&str] = &["save", "release", "deploy"];

fn parse_instance_sizes(sizes: &[&String]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let mut instance_sizes = serde_json::Map::new();
    for entry in sizes {
        let (id, size) = entry
            .split_once('=')
            .with_context(|| format!("--size '{}' must be <id>=<size>", entry))?;
        if id.is_empty() || size.is_empty() {
            bail!("--size '{}' must contain a non-empty id and size", entry);
        }
        if instance_sizes.contains_key(id) {
            bail!("--size specified more than once for id '{}'", id);
        }
        if !INSTANCE_SIZES.contains(&size) {
            bail!(
                "--size '{}': '{}' is not an instance size (one of {})",
                entry,
                size,
                INSTANCE_SIZES.join(", ")
            );
        }
        instance_sizes.insert(id.to_string(), serde_json::Value::String(size.to_string()));
    }
    Ok(instance_sizes)
}

#[derive(Debug)]
pub(crate) struct ResizeCommand;

impl ResizeCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ResizeCommand {
    fn command(&self) -> Command {
        command(
            "resize",
            "Set the instance size of one or more services/workers (saved for the next deploy unless --apply says otherwise)",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment the size applies to"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region the size applies to"),
        )
        .arg(
            Arg::new("size")
                .long("size")
                .required(true)
                .action(ArgAction::Append)
                .help(
                    "<service-or-worker>=<size>, repeatable. The component is its manifest name \
                     (e.g. billing-service, billing-worker); sizes: nano, micro, small, medium, \
                     large, xlarge, 2xlarge (e.g. --size billing-worker=large)",
                ),
        )
        .arg(
            Arg::new("apply")
                .long("apply")
                .value_parser(APPLY_MODES.to_vec())
                .default_value("save")
                .help(
                    "How far to take the change: save = record the size for the next deploy \
                     (default); release = also create a release; deploy = release and deploy now",
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
        let sizes: Vec<&String> = matches
            .get_many::<String>("size")
            .context("--size is required")?
            .collect();

        let instance_sizes = parse_instance_sizes(&sizes)?;

        let apply = matches
            .get_one::<String>("apply")
            .map(String::as_str)
            .unwrap_or("save");

        let body = serde_json::json!({
            "applicationId": application_id,
            "environment": environment,
            "region": region,
            "instanceSizes": instance_sizes,
            "apply": apply,
        });

        let url = format!("{}/instance-sizes", get_platform_management_api_url());
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            bail!("Failed to resize: {}", response.text().unwrap_or_default());
        }

        let result: ResizeResponse = response
            .json()
            .with_context(|| "Failed to parse resize response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        let headline = match result.applied.as_deref().unwrap_or(apply) {
            "deploy" => "  Deploying resize",
            "release" => "  Release created",
            _ => "  Instance sizes saved",
        };
        write!(stdout, "{}", headline)?;
        stdout.reset()?;
        writeln!(stdout)?;
        for entry in &result.updated {
            writeln!(
                stdout,
                "    {} -> {}",
                entry.component_name, entry.instance_type
            )?;
        }
        if let (Some(version), Some(id)) = (&result.release_version, &result.release_id) {
            writeln!(stdout, "  release {} ({})", version, id)?;
        }
        if let Some(deployment_id) = &result.deployment_id {
            writeln!(stdout, "  deployment {}", deployment_id)?;
        }
        writeln!(stdout, "  {}", result.message)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResizeResponse {
    message: String,
    #[serde(default)]
    applied: Option<String>,
    #[serde(default)]
    updated: Vec<ResizedComponent>,
    #[serde(default)]
    release_id: Option<String>,
    #[serde(default)]
    release_version: Option<String>,
    #[serde(default)]
    deployment_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResizedComponent {
    component_name: String,
    instance_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resize_cmd() -> Command {
        ResizeCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        resize_cmd().debug_assert();
    }

    #[test]
    fn requires_env_region_and_size() {
        assert!(resize_cmd().try_get_matches_from(["resize"]).is_err());
        assert!(
            resize_cmd()
                .try_get_matches_from([
                    "resize",
                    "-e",
                    "dev",
                    "-r",
                    "us-east-1",
                    "--size",
                    "svc-1=medium"
                ])
                .is_ok()
        );
    }

    #[test]
    fn size_accepts_multiple_entries() {
        let matches = resize_cmd()
            .try_get_matches_from([
                "resize",
                "-e",
                "dev",
                "-r",
                "us-east-1",
                "--size",
                "svc-1=medium",
                "--size",
                "worker-1=small",
            ])
            .unwrap();
        let sizes: Vec<&String> = matches.get_many::<String>("size").unwrap().collect();
        assert_eq!(sizes.len(), 2);
    }

    #[test]
    fn parse_instance_sizes_rejects_empty_value_and_duplicates() {
        let missing_eq = "svc-1".to_string();
        assert!(parse_instance_sizes(&[&missing_eq]).is_err());

        let empty_value = "svc-1=".to_string();
        assert!(parse_instance_sizes(&[&empty_value]).is_err());

        let empty_id = "=medium".to_string();
        assert!(parse_instance_sizes(&[&empty_id]).is_err());

        let dup_a = "svc-1=medium".to_string();
        let dup_b = "svc-1=large".to_string();
        assert!(parse_instance_sizes(&[&dup_a, &dup_b]).is_err());

        // An EC2 instance type is not an instance size.
        let not_a_size = "svc-1=t3.medium".to_string();
        let err = parse_instance_sizes(&[&not_a_size])
            .unwrap_err()
            .to_string();
        assert!(err.contains("not an instance size"), "{err}");

        let valid_a = "svc-1=medium".to_string();
        let valid_b = "worker-1=small".to_string();
        let parsed = parse_instance_sizes(&[&valid_a, &valid_b]).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.get("svc-1").unwrap(), "medium");
    }

    #[test]
    fn apply_defaults_to_save_and_only_accepts_known_modes() {
        let m = resize_cmd()
            .try_get_matches_from([
                "resize",
                "-e",
                "dev",
                "-r",
                "us-east-1",
                "--size",
                "w=large",
            ])
            .unwrap();
        assert_eq!(m.get_one::<String>("apply").unwrap(), "save");
        for mode in APPLY_MODES {
            assert!(
                resize_cmd()
                    .try_get_matches_from([
                        "resize",
                        "-e",
                        "dev",
                        "-r",
                        "us-east-1",
                        "--size",
                        "w=large",
                        "--apply",
                        mode
                    ])
                    .is_ok()
            );
        }
        assert!(
            resize_cmd()
                .try_get_matches_from([
                    "resize",
                    "-e",
                    "dev",
                    "-r",
                    "us-east-1",
                    "--size",
                    "w=large",
                    "--apply",
                    "yolo"
                ])
                .is_err()
        );
    }

    #[test]
    fn resize_response_deserializes_with_and_without_release_fields() {
        let deployed = r#"{
            "message": "Deployment triggered",
            "applied": "deploy",
            "updated": [{"componentName": "billing-worker", "instanceType": "large"}],
            "releaseId": "rel-1",
            "releaseVersion": "1.0.1",
            "deploymentId": "dep-1"
        }"#;
        let response: ResizeResponse = serde_json::from_str(deployed).unwrap();
        assert_eq!(response.release_version.as_deref(), Some("1.0.1"));
        assert_eq!(response.updated[0].instance_type, "large");

        let saved = r#"{"message": "saved", "applied": "save", "updated": []}"#;
        let response: ResizeResponse = serde_json::from_str(saved).unwrap();
        assert!(response.deployment_id.is_none());
        assert_eq!(response.applied.as_deref(), Some("save"));
    }
}
