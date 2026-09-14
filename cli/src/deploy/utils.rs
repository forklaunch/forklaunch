use std::{io::Write, thread::sleep, time::Duration};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use termcolor::{Color, StandardStream, WriteColor};

use crate::core::hmac::AuthMode;

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
    use crate::core::http_client;

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
            _ => {
                sleep(Duration::from_secs(3));
            }
        }
    }

    Ok(())
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
    fn unrecognized_phase_falls_back_to_raw_string() {
        // Locks in the defensive fallback: a resource-modify deployment emitting a
        // phase name not in this match (e.g. "modifying_database") must not panic
        // or print nothing — it should surface the raw phase string.
        assert_eq!(phase_message("modifying_database"), "modifying_database");
    }
}
