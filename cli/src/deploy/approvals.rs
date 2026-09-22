use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use serde_json::json;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

/// `forklaunch deploy approvals` — the deployment-approval gate.
///
/// An organization that requires approval for production deploys parks every
/// such deploy as `awaiting_approval` until an admin releases it. That includes
/// every managed-instance launch, reset and update on a gated org (the instance
/// shows `launchApprovalState: pending`), so an agent driving the managed
/// lifecycle needs these three commands as much as the dashboard's button.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentApproval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deployment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
}

fn dash(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

#[derive(Debug)]
pub(crate) struct ApprovalsCommand;

impl ApprovalsCommand {
    pub(crate) fn new() -> Self {
        Self
    }

    fn list(matches: &ArgMatches) -> Result<()> {
        require_auth()?;
        let mut query: Vec<String> = Vec::new();
        if let Some(app) = matches.get_one::<String>("application") {
            query.push(format!("applicationId={}", urlencoding::encode(app)));
        }
        if let Some(status) = matches.get_one::<String>("status") {
            query.push(format!("status={}", urlencoding::encode(status)));
        }
        let url = format!(
            "{}/deployment-approvals{}{}",
            get_platform_management_api_url(),
            if query.is_empty() { "" } else { "?" },
            query.join("&")
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list deployment approvals ({}): {}",
                response.status(),
                response.text().unwrap_or_default()
            );
        }
        let approvals: Vec<DeploymentApproval> = response
            .json()
            .with_context(|| "Failed to parse the deployment approvals")?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&approvals)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Deployment approvals")?;
        stdout.reset()?;
        writeln!(stdout)?;
        if approvals.is_empty() {
            writeln!(stdout, "  None match.")?;
            writeln!(stdout)?;
            return Ok(());
        }
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<38} {:<10} {:<12} {:<10} {:<38} {:<24} {}",
            "ID", "STATUS", "ENV", "REGION", "APPLICATION", "REQUESTED BY", "DEPLOYMENT"
        )?;
        stdout.reset()?;
        for a in &approvals {
            writeln!(
                stdout,
                "  {:<38} {:<10} {:<12} {:<10} {:<38} {:<24} {}",
                dash(&a.id),
                dash(&a.status),
                dash(&a.environment),
                dash(&a.region),
                dash(&a.application_id),
                dash(&a.requested_by),
                dash(&a.deployment_id),
            )?;
            if let Some(reason) = a.reason.as_deref() {
                writeln!(stdout, "      {}", reason)?;
            }
        }
        writeln!(stdout)?;
        log_info!(
            stdout,
            "A managed-instance launch on a gated organization parks here with requestedBy=system; approve it to let the instance advance."
        );
        Ok(())
    }

    fn decide(matches: &ArgMatches, verb: &str) -> Result<()> {
        require_auth()?;
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let mut body = serde_json::Map::new();
        if let Some(reason) = matches.get_one::<String>("reason") {
            body.insert("reason".into(), json!(reason));
        }
        let url = format!(
            "{}/deployment-approvals/{}/{}",
            get_platform_management_api_url(),
            urlencoding::encode(id),
            verb
        );
        if matches.get_flag("dryrun") {
            println!("[DRYRUN] POST {}", url);
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Object(body))?
            );
            println!("[DRYRUN] no request was sent.");
            return Ok(());
        }
        let response = http_client::post(&url, serde_json::Value::Object(body))
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to {} deployment approval ({}): {}",
                verb,
                response.status(),
                response.text().unwrap_or_default()
            );
        }
        let approval: DeploymentApproval = response
            .json()
            .with_context(|| "Failed to parse the deployment approval")?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&approval)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(
            stdout,
            "Approval {} {}: {} / {} on application {}",
            dash(&approval.id),
            dash(&approval.status),
            dash(&approval.environment),
            dash(&approval.region),
            dash(&approval.application_id)
        );
        if verb == "approve" {
            log_info!(
                stdout,
                "The parked deployment is dispatched now. A managed instance that was holding advances on its own once the deploy lands and answers on its host."
            );
        }
        writeln!(stdout)?;
        Ok(())
    }
}

impl CliCommand for ApprovalsCommand {
    fn command(&self) -> Command {
        command(
            "approvals",
            "List, approve or reject deployments parked by the approval gate",
        )
        .long_about(
            "List, approve or reject deployments parked by the approval gate.\n\n\
             An organization that requires approval for production deploys parks every\n\
             such deploy as awaiting_approval until an admin releases it. Managed-instance\n\
             launches, resets and updates on a gated organization park the same way (the\n\
             instance shows launchApprovalState=pending and requestedBy=system on the\n\
             approval); nothing advances until `approve`. Approving and rejecting need the\n\
             admin role.",
        )
        .subcommand(
            command("list", "List deployment approvals")
                .arg(
                    Arg::new("application")
                        .long("application")
                        .help("Only this application id"),
                )
                .arg(
                    Arg::new("status")
                        .long("status")
                        .value_parser(["pending", "approved", "rejected", "expired", "consumed"])
                        .help("Only approvals in this status"),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output raw JSON")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            command("approve", "Approve a parked deployment (admin)")
                .arg(
                    Arg::new("id")
                        .long("id")
                        .required(true)
                        .help("Approval id (from `approvals list`)"),
                )
                .arg(
                    Arg::new("reason")
                        .long("reason")
                        .help("Recorded with the decision"),
                )
                .arg(
                    Arg::new("dryrun")
                        .long("dryrun")
                        .help("Print the request without sending it")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output raw JSON")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(
            command("reject", "Reject a parked deployment (admin)")
                .arg(
                    Arg::new("id")
                        .long("id")
                        .required(true)
                        .help("Approval id (from `approvals list`)"),
                )
                .arg(
                    Arg::new("reason")
                        .long("reason")
                        .help("Recorded with the decision"),
                )
                .arg(
                    Arg::new("dryrun")
                        .long("dryrun")
                        .help("Print the request without sending it")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Output raw JSON")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub)) => Self::list(sub),
            Some(("approve", sub)) => Self::decide(sub, "approve"),
            Some(("reject", sub)) => Self::decide(sub, "reject"),
            _ => unreachable!(),
        }
    }
}
