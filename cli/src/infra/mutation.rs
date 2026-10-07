use std::io::Write;

use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    constants::{get_platform_ui_url, get_resource_management_api_url},
    core::{
        hmac::AuthMode,
        http_client::{patch, post},
    },
    deploy::utils::stream_deployment_status,
};

use super::{
    resource_resolver::encode_resource_id_for_url,
    types::{
        DeployResourceRequest, DeployResourceResponse, PatchResourceRequest, ResourceConfig,
        ResourceDetailResponse,
    },
};

/// Everything a mutating `fl infra` subcommand (`resize`, `config-set`) needs to
/// apply a change: the target resource, what changed, and how to apply it.
pub(crate) struct MutationRequest {
    pub(crate) resource_id: String,
    pub(crate) current: ResourceDetailResponse,
    pub(crate) requested_config: ResourceConfig,
    pub(crate) distribution_strategy: Option<String>,
    pub(crate) primary_region: Option<String>,
    pub(crate) snapshot_before_change: Option<bool>,
    pub(crate) skip_confirm: bool,
    pub(crate) dry_run: bool,
}

/// True if nothing was requested at all — no `ResourceConfig` field, no distribution
/// strategy, no primary region. Callers should bail on this immediately, before any
/// network call (resolving the resource, fetching its current detail), not after.
pub(crate) fn nothing_to_change(
    config: &ResourceConfig,
    distribution_strategy: &Option<String>,
    primary_region: &Option<String>,
) -> bool {
    is_metadata_only(config) && distribution_strategy.is_none() && primary_region.is_none()
}

/// True if the requested config has no `ResourceConfig` fields set at all — i.e. only
/// `distribution_strategy`/`primary_region` (or nothing) changed, which routes through
/// the cheaper synchronous `PATCH /:id` path instead of a full `POST /:id/deploy`.
fn is_metadata_only(config: &ResourceConfig) -> bool {
    let ResourceConfig {
        instance_class,
        engine,
        allocated_storage,
        num_cache_nodes,
        number_of_broker_nodes,
        ebs_storage_size,
        visibility_timeout,
        message_retention_seconds,
        port,
        multi_az,
        node_type,
        broker_node_type,
        kafka_version,
        queue_type,
        encryption,
    } = config;

    instance_class.is_none()
        && engine.is_none()
        && allocated_storage.is_none()
        && num_cache_nodes.is_none()
        && number_of_broker_nodes.is_none()
        && ebs_storage_size.is_none()
        && visibility_timeout.is_none()
        && message_retention_seconds.is_none()
        && port.is_none()
        && multi_az.is_none()
        && node_type.is_none()
        && broker_node_type.is_none()
        && kafka_version.is_none()
        && queue_type.is_none()
        && encryption.is_none()
}

/// Prints every field that would change: `current value -> requested value`.
fn print_config_diff(stdout: &mut StandardStream, current: &ResourceConfig, requested: &ResourceConfig) -> Result<()> {
    writeln!(stdout, "  The following will change:")?;
    macro_rules! diff_field {
        ($label:expr, $cur:expr, $req:expr) => {
            if let Some(new_value) = &$req {
                let cur_display = $cur
                    .as_ref()
                    .map(|v| format!("{:?}", v))
                    .unwrap_or_else(|| "unset".to_string());
                writeln!(
                    stdout,
                    "    {:<24} {} -> {:?}",
                    $label,
                    cur_display,
                    new_value
                )?;
            }
        };
    }
    diff_field!("instance_class:", current.instance_class, requested.instance_class);
    diff_field!("engine:", current.engine, requested.engine);
    diff_field!("allocated_storage:", current.allocated_storage, requested.allocated_storage);
    diff_field!("num_cache_nodes:", current.num_cache_nodes, requested.num_cache_nodes);
    diff_field!("number_of_broker_nodes:", current.number_of_broker_nodes, requested.number_of_broker_nodes);
    diff_field!("ebs_storage_size:", current.ebs_storage_size, requested.ebs_storage_size);
    diff_field!("visibility_timeout:", current.visibility_timeout, requested.visibility_timeout);
    diff_field!("message_retention_seconds:", current.message_retention_seconds, requested.message_retention_seconds);
    diff_field!("port:", current.port, requested.port);
    diff_field!("multi_az:", current.multi_az, requested.multi_az);
    diff_field!("node_type:", current.node_type, requested.node_type);
    diff_field!("broker_node_type:", current.broker_node_type, requested.broker_node_type);
    diff_field!("kafka_version:", current.kafka_version, requested.kafka_version);
    diff_field!("queue_type:", current.queue_type, requested.queue_type);
    diff_field!("encryption:", current.encryption, requested.encryption);
    Ok(())
}

/// Runs a mutation request end to end: prints the diff, gates on confirmation
/// (skippable), routes to the cheap synchronous PATCH path when nothing but
/// distribution metadata changed, otherwise calls `POST /deploy` and polls to
/// completion — printing the dashboard URL before polling starts so a dropped
/// connection never leaves the user unable to check on a live mutation.
pub(crate) fn run_mutation(req: MutationRequest) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if is_metadata_only(&req.requested_config) {
        if req.distribution_strategy.is_none() && req.primary_region.is_none() {
            bail!("nothing to change — no fields were set");
        }
        if req.dry_run {
            writeln!(stdout, "Dry run: would PATCH distributionStrategy/primaryRegion. No changes applied.")?;
            return Ok(());
        }
        let updated = patch_resource(
            &req.resource_id,
            req.distribution_strategy,
            req.primary_region,
        )?;
        writeln!(stdout, "Updated {} (status: {})", updated.name, updated.status)?;
        return Ok(());
    }

    writeln!(stdout)?;
    print_config_diff(&mut stdout, &req.current.manifest_config, &req.requested_config)?;
    writeln!(stdout)?;

    if req.dry_run {
        writeln!(stdout, "Dry run: no changes applied.")?;
        return Ok(());
    }

    if !req.skip_confirm {
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Apply this change?")
            .default(false)
            .interact()
            .with_context(|| "Failed to read confirmation")?;
        if !confirmed {
            writeln!(stdout, "Aborted — no changes applied.")?;
            return Ok(());
        }
    }

    let response = deploy_resource(
        &req.resource_id,
        Some(req.requested_config),
        req.distribution_strategy,
        req.primary_region,
        req.snapshot_before_change,
    )?;

    let dashboard_url = format!(
        "{}/dashboard/deployments/{}",
        get_platform_ui_url(),
        response.deployment_id
    );
    writeln!(stdout)?;
    stdout.set_color(termcolor::ColorSpec::new().set_fg(Some(Color::Green)))?;
    writeln!(stdout, "Change submitted: {}", dashboard_url)?;
    stdout.reset()?;
    writeln!(stdout, "(Check this URL if the live status stream below is interrupted.)")?;
    writeln!(stdout)?;

    // `fl infra` is JWT/session-only (resource-management has no HMAC support on
    // these routes); `stream_deployment_status` is shared with `deploy create`,
    // which still supports HMAC, so it takes an AuthMode — always JWT from here.
    stream_deployment_status(&AuthMode::Jwt, &response.deployment_id, None, None, &mut stdout)?;

    Ok(())
}

/// `POST /:id/deploy` — the unified endpoint backing both `resize` and `config-set`'s
/// general path. A 409 means another operation is already in progress for this
/// app/environment/region (the platform's deployment lock is scoped broader than the
/// single resource) — surfaced as a clear retry message instead of the raw body.
fn deploy_resource(
    resource_id: &str,
    manifest_config: Option<ResourceConfig>,
    distribution_strategy: Option<String>,
    primary_region: Option<String>,
    snapshot_before_change: Option<bool>,
) -> Result<DeployResourceResponse> {
    let url = format!(
        "{}/platform-resources/{}/deploy",
        get_resource_management_api_url(),
        encode_resource_id_for_url(resource_id)
    );

    let body = DeployResourceRequest {
        manifest_config,
        distribution_strategy,
        primary_region,
        snapshot_before_change,
    };

    let response = post(&url, serde_json::to_value(&body)?)
        .with_context(|| "Failed to reach resource-management API")?;
    let status = response.status();

    if status.as_u16() == 409 {
        bail!(
            "another operation is already in progress for this application/environment/region — try again shortly"
        );
    }

    if !status.is_success() {
        let body_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        bail!("resource-management API returned {} — {}", status, body_text);
    }

    response
        .json()
        .with_context(|| "Failed to parse deploy response")
}

/// `PATCH /:id` — synchronous, metadata-only (distribution strategy / primary region),
/// no deployment row created, no polling needed. Confirmed against the platform's
/// response schema (`ResourceDetailResponseSchema`, same shape as `GET /:resourceId`).
fn patch_resource(
    resource_id: &str,
    distribution_strategy: Option<String>,
    primary_region: Option<String>,
) -> Result<ResourceDetailResponse> {
    let url = format!(
        "{}/platform-resources/{}",
        get_resource_management_api_url(),
        encode_resource_id_for_url(resource_id)
    );

    let body = PatchResourceRequest {
        distribution_strategy,
        primary_region,
    };

    let response = patch(&url, serde_json::to_value(&body)?)
        .with_context(|| "Failed to reach resource-management API")?;
    let status = response.status();

    if !status.is_success() {
        let body_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        bail!("resource-management API returned {} — {}", status, body_text);
    }

    response
        .json()
        .with_context(|| "Failed to parse patch response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_only_true_when_all_config_fields_unset() {
        assert!(is_metadata_only(&ResourceConfig::default()));
    }

    #[test]
    fn nothing_to_change_true_when_everything_unset() {
        assert!(nothing_to_change(&ResourceConfig::default(), &None, &None));
    }

    #[test]
    fn nothing_to_change_false_when_distribution_strategy_set() {
        assert!(!nothing_to_change(
            &ResourceConfig::default(),
            &Some("centralized".to_string()),
            &None
        ));
    }

    #[test]
    fn metadata_only_false_when_any_config_field_set() {
        let config = ResourceConfig {
            instance_class: Some("db.t3.small".to_string()),
            ..Default::default()
        };
        assert!(!is_metadata_only(&config));
    }
}
