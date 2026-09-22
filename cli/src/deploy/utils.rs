use std::{
    io::Write,
    thread::sleep,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use termcolor::{Color, StandardStream, WriteColor};

use crate::core::{
    exit_code::{EXIT_AWAITING_APPROVAL, EXIT_STILL_RUNNING, ExitWith},
    hmac::AuthMode,
};

/// How long `stream_deployment_status` waits before it stops polling. A deploy
/// that is still running after this is reported as such (exit 3) rather than
/// polled forever; the platform keeps deploying regardless.
pub(crate) const DEFAULT_WAIT: Duration = Duration::from_secs(45 * 60);

#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentStatus {
    #[allow(dead_code)]
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) phase: Option<String>,
    pub(crate) endpoints: Option<DeploymentEndpoints>,
    #[serde(rename = "errorMessage")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentEndpoints {
    pub(crate) api: Option<String>,
    pub(crate) docs: Option<String>,
}

pub(crate) fn stream_deployment_status(
    auth_mode: &AuthMode,
    deployment_id: &str,
    environment: Option<&str>,
    region: Option<&str>,
    stdout: &mut StandardStream,
) -> Result<()> {
    stream_deployment_status_for(
        auth_mode,
        deployment_id,
        environment,
        region,
        DEFAULT_WAIT,
        stdout,
    )
}

/// Poll a deployment until it reaches a terminal state, or `max_wait` passes.
///
/// Exit codes, so a script or agent can tell the outcomes apart:
/// - `completed` → Ok
/// - `failed`, `cancelled`, `rolled_back` → Err (exit 1)
/// - `awaiting_approval` → Err with exit 2: the deploy is parked, not failed
/// - still running after `max_wait` → Err with exit 3
pub(crate) fn stream_deployment_status_for(
    auth_mode: &AuthMode,
    deployment_id: &str,
    environment: Option<&str>,
    region: Option<&str>,
    max_wait: Duration,
    stdout: &mut StandardStream,
) -> Result<()> {
    use crate::core::http_client;

    let started = Instant::now();

    let url = if auth_mode.is_hmac() {
        format!(
            "{}/internal/deployments/{}",
            crate::constants::get_platform_management_api_url(),
            deployment_id
        )
    } else {
        format!(
            "{}/deployments/{}",
            crate::constants::get_platform_management_api_url(),
            deployment_id
        )
    };
    let mut last_phase: Option<String> = None;

    loop {
        // Polling deployment status
        let response = http_client::get_with_auth(auth_mode, &url)?;

        if !response.status().is_success() {
            let response_text = response
                .text()
                .with_context(|| "Failed to read status response")?;
            bail!("Failed to get deployment status: {}", response_text);
        }

        let response_text = response
            .text()
            .with_context(|| "Failed to read status response")?;

        let status: DeploymentStatus = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse deployment status: {}", response_text))?;

        if let Some(phase) = &status.phase {
            if last_phase.as_ref() != Some(phase) {
                display_phase_update(phase, stdout)?;
                last_phase = Some(phase.clone());
            }
        }

        match status.status.as_str() {
            "completed" => {
                log_header!(stdout, Color::Green, "\nOperation successful!");

                if let Some(endpoints) = status.endpoints {
                    writeln!(stdout)?;
                    if let Some(api) = endpoints.api {
                        log_info!(stdout, "API: {}", api);
                    }
                    if let Some(docs) = endpoints.docs {
                        log_info!(stdout, "Docs: {}", docs);
                    }
                }
                break;
            }
            "failed" => {
                log_header!(stdout, Color::Red, "\nOperation failed");

                if let Some(error) = &status.error {
                    log_error!(stdout, "Error: {}", error);
                    // A synchronous deploy is the case where someone is actually
                    // watching. Making them run a second command to find out
                    // which component wanted which key — and then work out what
                    // to do about it alone — is where the session dies. Show the
                    // fix and a prompt they can hand to an agent, here.
                    crate::deploy::info::print_remediation(
                        stdout, error, environment, region,
                    )?;
                }

                // Carry the reason into the error itself. `bail!("Operation
                // failed")` put the cause on stdout and nothing in the error, so
                // any caller reading only the failure — CI, or an agent reading
                // the last line — lost it entirely.
                match &status.error {
                    Some(error) => bail!("Deployment failed: {}", error),
                    None => bail!("Deployment failed (no reason reported)"),
                }
            }
            "cancelled" => {
                log_header!(stdout, Color::Yellow, "\n[CANCELLED] Deployment was cancelled");
                if let Some(error) = status.error {
                    log_info!(stdout, "{}", error);
                }
                bail!("Deployment cancelled");
            }
            "rolled_back" => {
                // The platform undid this deploy. The application is on the
                // previous release, which is a failure of *this* deploy even
                // though nothing is broken.
                log_header!(stdout, Color::Red, "\nDeployment rolled back");
                if let Some(error) = &status.error {
                    log_error!(stdout, "Reason: {}", error);
                }
                match &status.error {
                    Some(error) => bail!("Deployment rolled back: {}", error),
                    None => bail!("Deployment rolled back (no reason reported)"),
                }
            }
            "awaiting_approval" => {
                // Parked, not failed. Approval can take hours, so waiting here
                // would look like a hang to a person and a timeout to a script.
                log_header!(stdout, Color::Yellow, "\nDeployment is awaiting approval");
                log_info!(
                    stdout,
                    "An organization admin can release it with: forklaunch deploy approvals approve <approval-id>"
                );
                log_info!(stdout, "List pending approvals: forklaunch deploy approvals list");
                log_info!(
                    stdout,
                    "Follow this deployment: forklaunch deploy info --deployment {}",
                    deployment_id
                );
                return Err(ExitWith::new(
                    EXIT_AWAITING_APPROVAL,
                    format!(
                        "Deployment {} is awaiting approval (exit 2, not a failure)",
                        deployment_id
                    ),
                ));
            }
            _ => {
                if started.elapsed() >= max_wait {
                    let minutes = max_wait.as_secs() / 60;
                    log_header!(stdout, Color::Yellow, "\nStopped waiting");
                    log_info!(
                        stdout,
                        "The deployment is still {} after {} minutes. It continues on the platform; this exit says nothing about its outcome.",
                        status.status,
                        minutes
                    );
                    log_info!(
                        stdout,
                        "Follow it: forklaunch deploy info --deployment {}",
                        deployment_id
                    );
                    return Err(ExitWith::new(
                        EXIT_STILL_RUNNING,
                        format!(
                            "Deployment {} still {} after {} minutes (exit 3, outcome unknown)",
                            deployment_id, status.status, minutes
                        ),
                    ));
                }
                sleep(Duration::from_secs(3));
            }
        }
    }

    Ok(())
}

/// Every status the platform can report, and whether the CLI treats it as
/// terminal. Kept next to the match above so a new platform status shows up
/// here first.
#[cfg(test)]
pub(crate) fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "cancelled" | "rolled_back" | "awaiting_approval"
    )
}

/// Maps a deployment phase string to a human-readable message. Unrecognized phases
/// (e.g. new resource-modify phases like "modifying_database") fall back to the raw
/// phase string rather than panicking or dropping the update silently.
fn phase_message(phase: &str) -> &str {
    match phase {
        "validating" => "  Validating configuration...",
        "provisioning_database" => "  Provisioning database (RDS PostgreSQL db.t3.micro)...",
        "provisioning_cache" => "  Provisioning cache (ElastiCache Redis)...",
        "creating_network" => "  Creating network infrastructure...",
        "creating_load_balancer" => "  Creating load balancer...",
        "deploying_services" => "  Deploying services (256m CPU, 512Mi RAM)...",
        "configuring_autoscaling" => "  Configuring auto-scaling (1-2 replicas)...",
        "configuring_monitoring" => "  Setting up monitoring (OTEL, Prometheus, Grafana)...",
        "destroying_services" => "  Destroying services...",
        "destroying_load_balancer" => "  Destroying load balancer...",
        "destroying_network" => "  Destroying network infrastructure...",
        "destroying_cache" => "  Destroying cache...",
        "destroying_database" => "  Destroying database...",
        _ => phase,
    }
}

fn display_phase_update(phase: &str, stdout: &mut StandardStream) -> Result<()> {
    log_info!(stdout, "{}", phase_message(phase));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_phase_maps_to_human_message() {
        assert_eq!(phase_message("validating"), "  Validating configuration...");
    }

    #[test]
    fn every_platform_status_is_classified() {
        // Mirrors DeploymentStatusEnum in forklaunch-platform. `queued`,
        // `pending`, `provisioning` and `deploying` are the in-flight states the
        // poll keeps waiting on; everything else must end the wait, or a script
        // hangs on a parked or rolled-back deploy (Main Street, Sept 2026).
        for s in ["completed", "failed", "cancelled", "rolled_back", "awaiting_approval"] {
            assert!(is_terminal_status(s), "{s} must end the wait");
        }
        for s in ["queued", "pending", "provisioning", "deploying"] {
            assert!(!is_terminal_status(s), "{s} must keep waiting");
        }
    }

    #[test]
    fn unrecognized_phase_falls_back_to_raw_string() {
        // Locks in the defensive fallback: a resource-modify deployment emitting a
        // phase name not in this match (e.g. "modifying_database") must not panic
        // or print nothing — it should surface the raw phase string.
        assert_eq!(phase_message("modifying_database"), "modifying_database");
    }
}
