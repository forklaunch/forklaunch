use anyhow::{Context, Result, anyhow, bail};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::{
    constants::get_resource_management_api_url,
    core::{http_client::get, manifest::application::ApplicationManifestData},
};

use super::types::{ApplicationResourcesResponse, ResourceDetailResponse, ResourceListItem};

/// Characters that must be escaped in a URL path segment. Letters, digits, and the
/// unreserved punctuation used by UUIDs (`-`, `_`, `.`, `~`) are left untouched, so
/// normal platform-resolved ids are byte-for-byte unchanged. Only characters that
/// would otherwise restructure the request (`/`, `?`, `#`, `%`, whitespace, etc.)
/// get encoded.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// Percent-encodes a resource id before it's interpolated into a URL path segment.
/// Every `.../platform-resources/{resource_id}...` URL must go through this — the id
/// can come from the platform (always a clean UUID, effectively a no-op here) or
/// from the user-supplied `--resource-id` escape hatch, which is unvalidated input
/// and must not be trusted as URL structure (a value containing `/`, `?`, `#`, or
/// `..` should stay opaque data, not alter the request path).
pub(crate) fn encode_resource_id_for_url(resource_id: &str) -> String {
    utf8_percent_encode(resource_id, PATH_SEGMENT).to_string()
}

/// Maps the CLI's friendly resource-type token (matching `ResourceInventory`'s field
/// names) to the platform's exact `IntegrationType` string.
pub(crate) fn resource_type_to_integration_type(token: &str) -> Result<&'static str> {
    match token {
        "database" => Ok("database"),
        "cache" => Ok("cache"),
        "queue" => Ok("messagequeue"),
        "object-store" => Ok("objectstore"),
        other => Err(anyhow!(
            "unknown resource type '{other}' — expected one of: database, cache, queue, object-store"
        )),
    }
}

/// Fetches every provisioned resource for the application in a given environment.
/// Shared by `fl infra list` and every resolving subcommand — do not duplicate this
/// call elsewhere; both call sites must stay on this one function.
fn application_resources_url(application_id: &str, environment: &str) -> String {
    format!(
        "{}/platform-resources/application/{}?environment={}",
        get_resource_management_api_url(),
        encode_resource_id_for_url(application_id),
        urlencoding::encode(environment)
    )
}

pub(crate) fn fetch_application_resources(
    application_id: &str,
    environment: &str,
) -> Result<Vec<ResourceListItem>> {
    let url = application_resources_url(application_id, environment);

    let response = get(&url).with_context(|| "Failed to reach resource-management API")?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        bail!("resource-management API returned {} — {}", status, body);
    }

    let parsed: ApplicationResourcesResponse = response
        .json()
        .with_context(|| "Failed to parse resource list response")?;
    Ok(parsed.resources)
}

/// Fetches the full detail (including `manifestConfig`) for a resolved resource id.
/// Shared by `status`'s full view, `status --config`'s narrower view, and the
/// mutation commands' pre-change diff — all are the same underlying `GET
/// /:resourceId` call, differing only in what they do with the result.
pub(crate) fn fetch_resource_detail(resource_id: &str) -> Result<ResourceDetailResponse> {
    let url = format!(
        "{}/platform-resources/{}",
        get_resource_management_api_url(),
        encode_resource_id_for_url(resource_id)
    );

    let response = get(&url).with_context(|| "Failed to reach resource-management API")?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        bail!("resource-management API returned {} — {}", status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse resource detail response")
}

/// Fetches CloudWatch utilization metrics (CPU%, memory%, connection count) for a
/// resolved resource id — the series backing `fl infra status --metrics`.
pub(crate) fn fetch_resource_metrics(
    resource_id: &str,
) -> Result<Vec<super::types::MetricSeries>> {
    let url = format!(
        "{}/platform-resources/{}/metrics",
        get_resource_management_api_url(),
        encode_resource_id_for_url(resource_id)
    );

    let response = get(&url).with_context(|| "Failed to reach resource-management API")?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        bail!("resource-management API returned {} — {}", status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse resource metrics response")
}

/// A resolved `<project>:<type>` identifier, ready to address a specific platform
/// resource.
pub(crate) struct ResolvedResource {
    pub(crate) id: String,
}

/// Resolves a `<project-name>:<resource-type>` identifier (or an explicit
/// `--resource-id` override) to a concrete platform resource id.
///
/// Order: parse the token (no network) -> confirm the manifest actually configures
/// that resource for that project (no network) -> fetch and filter against the
/// platform's live resource list (network) -> disambiguate.
pub(crate) fn resolve(
    manifest: &ApplicationManifestData,
    application_id: &str,
    environment: &str,
    resource_arg: &str,
    resource_id_override: Option<&str>,
) -> Result<ResolvedResource> {
    let (project_name, resource_type) = resource_arg
        .split_once(':')
        .ok_or_else(|| anyhow!("expected '<project-name>:<resource-type>', got '{resource_arg}'"))?;

    let mapped_type = resource_type_to_integration_type(resource_type)?;

    if let Some(id) = resource_id_override {
        return Ok(ResolvedResource { id: id.to_string() });
    }

    let project = manifest
        .projects
        .iter()
        .find(|p| p.name == project_name)
        .ok_or_else(|| anyhow!("project '{project_name}' not found in manifest.toml"))?;

    let configured = project.resources.as_ref().map_or(false, |r| match resource_type {
        "database" => r.database.is_some(),
        "cache" => r.cache.is_some(),
        "queue" => r.queue.is_some(),
        "object-store" => r.object_store.is_some(),
        _ => false,
    });

    if !configured {
        bail!(
            "project '{project_name}' has no '{resource_type}' resource configured in manifest.toml"
        );
    }

    let resources = fetch_application_resources(application_id, environment)?;
    let matches: Vec<&ResourceListItem> = resources
        .iter()
        .filter(|r| {
            r.service_name.as_deref() == Some(project_name) && r.r#type == mapped_type
        })
        .collect();

    match matches.len() {
        0 => {
            let available = resources
                .iter()
                .map(|r| format!("{}:{}", r.service_name.as_deref().unwrap_or("?"), r.r#type))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "no '{resource_type}' resource found for project '{project_name}' in environment '{environment}'. \
                 Available resources: {}",
                if available.is_empty() { "none".to_string() } else { available }
            );
        }
        1 => Ok(ResolvedResource {
            id: matches[0].id.clone(),
        }),
        _ => {
            let candidates = matches
                .iter()
                .map(|r| {
                    format!(
                        "  id={} region={} status={}",
                        r.id,
                        r.region.as_deref().unwrap_or("?"),
                        r.status
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "multiple '{resource_type}' resources found for project '{project_name}':\n{}\n\
                 Use --resource-id <id> to disambiguate.",
                candidates
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_type_maps_all_known_tokens() {
        assert_eq!(resource_type_to_integration_type("database").unwrap(), "database");
        assert_eq!(resource_type_to_integration_type("cache").unwrap(), "cache");
        assert_eq!(resource_type_to_integration_type("queue").unwrap(), "messagequeue");
        assert_eq!(
            resource_type_to_integration_type("object-store").unwrap(),
            "objectstore"
        );
    }

    #[test]
    fn resource_type_rejects_unknown_token() {
        let err = resource_type_to_integration_type("bogus").unwrap_err();
        assert!(err.to_string().contains("unknown resource type"));
    }

    #[test]
    fn resource_type_rejects_raw_platform_string() {
        // "queue" (CLI-facing) maps to "messagequeue" (platform) — the raw platform
        // string itself should not also be accepted as a CLI-facing token, to avoid
        // two spellings meaning the same thing.
        assert!(resource_type_to_integration_type("messagequeue").is_err());
    }

    #[test]
    fn application_resources_url_encodes_application_id_and_environment() {
        let url = application_resources_url("app/../secret", "dev&environment=production");
        assert!(url.contains("app%2F..%2Fsecret"));
        assert!(url.ends_with("environment=dev%26environment%3Dproduction"));
        // exactly one `environment=` — a raw `&` in the value must not smuggle a
        // second query parameter in.
        assert_eq!(url.matches("environment=").count(), 1);
    }

    #[test]
    fn encode_resource_id_is_a_no_op_for_a_normal_uuid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(encode_resource_id_for_url(uuid), uuid);
    }

    #[test]
    fn encode_resource_id_escapes_path_separator() {
        assert_eq!(
            encode_resource_id_for_url("abc/../secret"),
            "abc%2F..%2Fsecret"
        );
    }

    #[test]
    fn encode_resource_id_escapes_query_and_fragment_markers() {
        let encoded = encode_resource_id_for_url("id?environment=prod#frag");
        assert!(!encoded.contains('?'));
        assert!(!encoded.contains('#'));
    }
}
