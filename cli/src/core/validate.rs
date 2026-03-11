use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use clap::ArgMatches;
use serde::Deserialize;

use super::base_path::{RequiredLocation, find_app_root_path};
use super::hmac::AuthMode;
use super::manifest::application::ApplicationManifestData;
use super::token::get_token;
use crate::constants::{get_billing_api_url, is_dev_build};

/// Validates user is authenticated. Returns the auth token.
pub(crate) fn require_auth() -> Result<String> {
    get_token()
}

/// Resolves auth mode: HMAC if env var is set, else JWT (validating token exists).
pub(crate) fn resolve_auth() -> Result<AuthMode> {
    let mode = AuthMode::detect();
    if matches!(mode, AuthMode::Jwt) {
        get_token()?; // validate JWT exists
    }
    Ok(mode)
}

/// Validates manifest exists and parses it. Returns (app_root, manifest).
pub(crate) fn require_manifest(
    matches: &ArgMatches,
) -> Result<(PathBuf, ApplicationManifestData)> {
    let (app_root, _) = find_app_root_path(matches, RequiredLocation::Application)?;
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read manifest at {:?}", manifest_path))?;
    let manifest: ApplicationManifestData =
        toml::from_str(&content).with_context(|| "Failed to parse manifest.toml")?;
    Ok((app_root, manifest))
}

#[derive(Deserialize)]
struct TrialStatusResponse {
    #[serde(rename = "isActive")]
    is_active: bool,
    #[serde(rename = "hasSubscription")]
    has_subscription: bool,
}

/// Validates that the account has an active trial or subscription.
/// Requires JWT auth mode (skipped for HMAC/internal calls and dev builds).
pub(crate) fn require_active_account(auth_mode: &AuthMode) -> Result<()> {
    // Skip for HMAC-authenticated (internal/CI) calls
    if matches!(auth_mode, AuthMode::Hmac { .. }) {
        return Ok(());
    }

    // Skip for dev builds — local IAM may not have subscription data
    if is_dev_build() {
        return Ok(());
    }

    let token = get_token()?;
    let api_url = get_billing_api_url();
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/trial/status", api_url))
        .header("Authorization", format!("Bearer {}", token))
        .send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(status) = resp.json::<TrialStatusResponse>() {
                if status.is_active || status.has_subscription {
                    return Ok(());
                }
            }
            bail!("Your free trial has expired. Please upgrade at https://forklaunch.com/checkout?plan=pro to continue using the CLI.");
        }
        Ok(resp) => {
            // Non-success status — allow through to avoid blocking on transient errors
            eprintln!(
                "Warning: Could not verify account status (HTTP {}). Proceeding.",
                resp.status()
            );
            Ok(())
        }
        Err(_) => {
            // Network error — allow through to avoid blocking on connectivity issues
            eprintln!("Warning: Could not reach account verification service. Proceeding.");
            Ok(())
        }
    }
}

/// Validates app is integrated with platform. Returns the application ID.
pub(crate) fn require_integration(manifest: &ApplicationManifestData) -> Result<String> {
    manifest
        .platform_application_id
        .clone()
        .ok_or_else(|| {
            anyhow!("Application not integrated with platform.\nRun: forklaunch integrate --app <app-id>")
        })
}
