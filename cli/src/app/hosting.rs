//! `forklaunch app hosting` — read and change where an application runs.
//!
//! Placement (`platform-shared` | `org-shared` | `dedicated`) used to be
//! write-once: settable at `app create`, enforced at the first-deploy gate, and
//! changeable nowhere. It was also unreadable — the application read API did not
//! return it — so a user could not even find out what they had chosen, let alone
//! switch. This command is both halves.
//!
//! The cost of a change is not uniform, and the control plane enforces the
//! difference rather than this command guessing at it:
//!
//! - Before any infrastructure exists, placement is a record and the change is
//!   instant and free.
//! - Afterwards the application's data lives on a substrate, so moving it runs
//!   as a migration with a cutover, and requires `--confirm-downtime`.

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

const CLUSTER_TYPES: [&str; 3] = ["platform-shared", "org-shared", "dedicated"];
const FRAMEWORKS: [&str; 5] = ["HIPAA", "PCI-DSS", "SOC 2", "GDPR", "CCPA"];

#[derive(Debug, Deserialize)]
struct PlacementResponse {
    #[serde(rename = "clusterType")]
    cluster_type: Option<String>,
    #[serde(rename = "complianceFrameworks")]
    compliance_frameworks: Option<Vec<String>>,
    #[serde(rename = "defaultHostingType")]
    default_hosting_type: Option<String>,
    #[serde(rename = "changeIsFree")]
    change_is_free: bool,
    #[serde(rename = "migrationId")]
    migration_id: Option<String>,
    message: String,
}

#[derive(Debug)]
pub(crate) struct HostingCommand;

impl HostingCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

fn render(stdout: &mut StandardStream, placement: &PlacementResponse) -> Result<()> {
    writeln!(stdout)?;
    log_header!(stdout, Color::Cyan, "Hosting");
    writeln!(stdout)?;
    writeln!(
        stdout,
        "  Cluster placement   {}",
        placement
            .cluster_type
            .as_deref()
            .unwrap_or("not chosen — the first deploy will ask")
    )?;
    writeln!(
        stdout,
        "  Resolved host       {}",
        placement
            .default_hosting_type
            .as_deref()
            .unwrap_or("ecs-fargate (default)")
    )?;
    // "not declared" and "declared as none" are different claims: an undeclared
    // application is unconstrained, not certified unregulated.
    writeln!(
        stdout,
        "  Compliance scope    {}",
        match placement.compliance_frameworks.as_ref() {
            Some(frameworks) if !frameworks.is_empty() => frameworks.join(", "),
            _ => String::from("not declared"),
        }
    )?;
    writeln!(stdout)?;
    Ok(())
}

impl CliCommand for HostingCommand {
    fn command(&self) -> Command {
        command(
            "hosting",
            "Show or change where this application's compute runs",
        )
        .arg(
            Arg::new("cluster_type")
                .long("cluster-type")
                .value_parser(CLUSTER_TYPES)
                .help(
                    "Move the application to this placement. Free before the first deploy; \
                     a data migration afterwards",
                ),
        )
        .arg(
            Arg::new("compliance_framework")
                .long("compliance-framework")
                .action(ArgAction::Append)
                .value_parser(FRAMEWORKS)
                .help(
                    "Replace the frameworks this application is scoped to; repeatable. \
                     Compliance-scoped apps cannot run on cross-tenant compute",
                ),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .help("Region the change applies to (required when changing anything)"),
        )
        .arg(
            Arg::new("confirm_downtime")
                .long("confirm-downtime")
                .action(ArgAction::SetTrue)
                .help(
                    "Acknowledge that moving an already-deployed application takes it \
                     offline during cutover",
                ),
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

        require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let api_url = get_platform_management_api_url();

        let cluster_type = matches.get_one::<String>("cluster_type").cloned();
        let frameworks: Option<Vec<String>> = matches
            .get_many::<String>("compliance_framework")
            .map(|values| values.cloned().collect());

        // No change requested: this is a read.
        if cluster_type.is_none() && frameworks.is_none() {
            let response = http_client::get(&format!(
                "{}/applications/{}/placement",
                api_url, application_id
            ))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
            if !response.status().is_success() {
                bail!(
                    "Failed to read hosting ({}): {}",
                    response.status(),
                    response.text().unwrap_or_default()
                );
            }
            let placement: PlacementResponse = response
                .json()
                .with_context(|| "Failed to parse placement response")?;
            render(&mut stdout, &placement)?;
            if placement.change_is_free {
                log_info!(
                    stdout,
                    "Nothing is deployed yet, so changing this is instant and costs nothing."
                );
            } else {
                log_info!(
                    stdout,
                    "This application is deployed. Changing placement migrates its data — pass --confirm-downtime."
                );
            }
            return Ok(());
        }

        let Some(region) = matches.get_one::<String>("region").cloned() else {
            bail!(
                "--region is required when changing hosting: a cluster can be available in one region and not another."
            );
        };

        let mut body = serde_json::json!({ "region": region });
        if let Some(cluster_type) = &cluster_type {
            body["clusterType"] = serde_json::json!(cluster_type);
        }
        if let Some(frameworks) = &frameworks {
            body["complianceFrameworks"] = serde_json::json!(frameworks);
        }
        if matches.get_flag("confirm_downtime") {
            body["confirmDowntime"] = serde_json::json!(true);
        }

        let response = http_client::patch(
            &format!("{}/applications/{}/placement", api_url, application_id),
            body,
        )
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if !status.is_success() {
            // 422 is the downtime acknowledgement and the no-op case; 409 is a
            // placement the app may not use, or a migration already running.
            // All three carry a message written to be read by a human, so relay
            // it rather than wrapping it in a generic failure.
            bail!("{}", response.text().unwrap_or_default());
        }

        let placement: PlacementResponse = response
            .json()
            .with_context(|| "Failed to parse placement response")?;

        render(&mut stdout, &placement)?;
        if let Some(migration_id) = &placement.migration_id {
            log_warn!(stdout, "{}", placement.message);
            log_info!(stdout, "Migration id: {}", migration_id);
            log_info!(
                stdout,
                "The application stays up until cutover. Track it in the dashboard."
            );
        } else {
            log_ok!(stdout, "{}", placement.message);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CliCommand;

    /// `propagate_version(true)` requires a version on the root command, which
    /// only exists once this is mounted under `forklaunch`. Supply one so the
    /// subcommand can be exercised on its own.
    fn hosting_cmd() -> Command {
        HostingCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        hosting_cmd().debug_assert();
    }

    /// With no flags this is a read, not an empty change. Getting that wrong
    /// would send a PATCH with nothing in it on every plain `app hosting`.
    #[test]
    fn no_flags_means_read() {
        let matches = hosting_cmd()
            .try_get_matches_from(vec!["hosting"])
            .unwrap();
        assert!(matches.get_one::<String>("cluster_type").is_none());
        assert!(matches.get_many::<String>("compliance_framework").is_none());
    }

    #[test]
    fn compliance_frameworks_are_repeatable() {
        let matches = hosting_cmd()
            .try_get_matches_from(vec![
                "hosting",
                "--compliance-framework",
                "HIPAA",
                "--compliance-framework",
                "SOC 2",
                "--region",
                "us-west-2",
            ])
            .unwrap();
        let frameworks: Vec<&String> = matches
            .get_many::<String>("compliance_framework")
            .unwrap()
            .collect();
        assert_eq!(frameworks, vec!["HIPAA", "SOC 2"]);
    }

    #[test]
    fn an_unknown_cluster_type_is_refused_before_the_network() {
        assert!(
            hosting_cmd()
                .try_get_matches_from(vec!["hosting", "--cluster-type", "serverless"])
                .is_err()
        );
    }
}
