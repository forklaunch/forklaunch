use anyhow::{Context, Result, bail};
use reqwest::blocking::Response;
use serde::Deserialize;
use serde_json::Value;

use super::types::ManagedInstance;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        hmac::AuthMode,
        http_client::{
            delete, get_with_auth, patch, post, post_unauthenticated, post_with_auth, put,
        },
        validate::resolve_auth,
    },
};

/// Every managed-apps command talks to platform-management's `/managed-mode` router and
/// never to the managed-apps service directly. That service's own design rule is "the
/// CLI and the dashboard both call platform-management, never managed-apps directly" —
/// platform-management is the control plane that owns organization scoping, role
/// checks, and auditing for these operations.
const MANAGED_MODE_BASE: &str = "/managed-mode";

/// Shown whenever the control plane answers a managed-apps route with 404. A 404 here
/// means the route is not mounted, which in practice means the platform predates the
/// managed-apps control-plane API rather than that the user did anything wrong.
pub(super) const UNSUPPORTED_CONTROL_PLANE: &str = "this control plane does not support managed apps yet — upgrade the platform \
     (the /managed-mode API this CLI needs is not mounted on the host it is pointed at)";

pub(super) fn managed_url(path: &str) -> String {
    format!(
        "{}{}{}",
        get_platform_management_api_url(),
        MANAGED_MODE_BASE,
        path
    )
}

/// Managed-apps commands are session-scoped on purpose. Templates and instances hang
/// off the calling user's organization, and HMAC/CI credentials carry no organization
/// identity for the control plane to scope them to — so rather than silently acting on
/// the wrong organization, refuse with an explanation.
pub(super) fn resolve_managed_auth() -> Result<AuthMode> {
    let auth_mode = resolve_auth()?;
    if auth_mode.is_hmac() {
        bail!(
            "managed commands require user/session auth — run `forklaunch login` first. \
             HMAC (CI) credentials carry no organization identity, so the control plane \
             cannot tell which organization's templates and instances to act on."
        );
    }
    Ok(auth_mode)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedModeSummary {
    #[serde(default)]
    pub(super) available: Option<bool>,
    #[serde(default)]
    pub(super) unavailable_reason: Option<String>,
    #[serde(default)]
    pub(super) instances: Vec<ManagedInstance>,
}

impl ManagedModeSummary {
    /// Treats a missing `available` field as available. A partial control-plane build
    /// may omit it, and refusing to run in that case would be a worse failure than
    /// letting the real request return its own error.
    fn is_available(&self) -> bool {
        self.available.unwrap_or(true)
    }
}

/// Fetches `/managed-mode/summary`, failing loudly when managed apps is either
/// unsupported by this control plane or not configured for this deployment.
///
/// Every subcommand calls this before its real request. That costs one extra round trip
/// and buys the difference between "you have no templates" and "managed apps was never
/// wired up here" — an empty list would otherwise read as the former when it is really
/// the latter. The parsed summary is returned rather than discarded because it already
/// carries the instance list, which `managed instance list` reuses.
pub(super) fn require_managed_mode(auth_mode: &AuthMode) -> Result<ManagedModeSummary> {
    let url = managed_url("/summary");
    let response = get_with_auth(auth_mode, &url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let status = response.status();

    if status.as_u16() == 404 {
        bail!("{}", UNSUPPORTED_CONTROL_PLANE);
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        bail!(
            "not authorized to use managed apps ({}) — run `forklaunch login`, and check that your organization has managed apps enabled",
            status
        );
    }
    if !status.is_success() {
        bail!(
            "control plane returned {} for {} — {}",
            status,
            url,
            body_snippet(response)
        );
    }

    let summary: ManagedModeSummary = response
        .json()
        .with_context(|| "Failed to parse the managed mode summary response")?;

    if !summary.is_available() {
        // The platform returns 200 with available:false for both "MANAGED_APPS_URL is
        // unset" and "managed-apps is unreachable". Print its reason verbatim — it is
        // more specific than anything this CLI could infer.
        let reason = summary
            .unavailable_reason
            .as_deref()
            .unwrap_or("managed apps is not configured for this deployment")
            .to_string();
        bail!(
            "managed apps is not available on this control plane: {}",
            reason
        );
    }

    Ok(summary)
}

/// What a 404 should mean for a given call.
pub(super) enum Missing {
    /// A collection-level route, so a 404 can only mean the endpoint is absent.
    Endpoint,
    /// A route addressing one record, so a 404 is ambiguous: either the record does not
    /// exist, or the endpoint is not implemented on this control plane. Say both.
    Resource(String),
    /// A route whose caller has a better account of what a 404 means than "not found"
    /// does. Used by `instance claim`: the control plane answers one deliberately
    /// indistinguishable 404 for a bad token, an expired token, an already-claimed
    /// instance, and an unknown id, so that an attacker cannot probe which links are
    /// live. The CLI must not invent a distinction the server refused to make — it
    /// lists the possibilities instead.
    Custom(String),
}

pub(super) fn ensure_success(response: Response, missing: Missing) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    match status.as_u16() {
        404 => {
            let snippet = body_snippet(response);
            // A 404 from a route that exists carries the platform's own explanation; a
            // 404 from a route that was never mounted carries the framework's default
            // ("Cannot POST /managed-mode/templates"). Distinguishing them is what keeps
            // "you asked for something that isn't there" from being reported as "your
            // platform is too old", and vice versa.
            if is_unrouted_404(&snippet) {
                bail!("{}", UNSUPPORTED_CONTROL_PLANE);
            }
            match missing {
                Missing::Endpoint => bail!("{}", UNSUPPORTED_CONTROL_PLANE),
                Missing::Resource(what) => bail!("{} not found — {}", what, snippet),
                Missing::Custom(message) => bail!("{}", message),
            }
        }
        // 503 is the control plane's own "managed-apps is unreachable" signal on the
        // proxy routes, distinct from a 404 meaning the route is missing.
        503 => bail!(
            "managed apps is temporarily unavailable — {}",
            body_snippet(response)
        ),
        401 | 403 => bail!("not authorized ({}) — {}", status, body_snippet(response)),
        409 => bail!("conflict — {}", body_snippet(response)),
        // The control plane validates enum-ish fields itself and answers 400 with the
        // allowed values (for example "status must be one of: draft, published,
        // retired"). Surface that body verbatim — it is more useful than anything this
        // CLI would restate, and wrapping it in "control plane returned 400" only pushes
        // the actionable part further right.
        400 => bail!("{}", body_snippet(response)),
        _ => bail!(
            "control plane returned {} — {}",
            status,
            body_snippet(response)
        ),
    }
}

/// Recognizes the "no route matched" 404 that Express-style servers generate, as
/// opposed to a 404 a real handler chose to return.
fn is_unrouted_404(snippet: &str) -> bool {
    let snippet = snippet.trim_start();
    ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .iter()
        .any(|method| snippet.starts_with(&format!("Cannot {} /", method)))
}

/// Pulls a human-readable message out of a response body. Control-plane errors are
/// sometimes `{ "message": "..." }` and sometimes a bare string, so try JSON first and
/// fall back to the raw text, trimmed so a stray HTML error page cannot flood the
/// terminal.
pub(super) fn body_snippet(response: Response) -> String {
    let text = response
        .text()
        .unwrap_or_else(|_| "unknown error".to_string());
    snippet_from_text(&text)
}

fn snippet_from_text(text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Some(string) = value.as_str() {
            return string.to_string();
        }
        for key in ["message", "error", "detail"] {
            if let Some(message) = value.get(key).and_then(Value::as_str) {
                return message.to_string();
            }
        }
    }

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }
    if trimmed.chars().count() > 400 {
        let truncated: String = trimmed.chars().take(400).collect();
        return format!("{}…", truncated);
    }
    trimmed.to_string()
}

/// Pulls the array out of a list response.
///
/// The control-plane managed-apps routes are being written in parallel with this CLI,
/// and list endpoints on platform-management are inconsistent about whether they return
/// a bare array or wrap it (`{ "templates": [...] }`, `{ "data": [...] }`). Accepting
/// either shape means a wrapper-key change on the server does not silently turn into
/// "you have none" here.
pub(super) fn extract_list<T: serde::de::DeserializeOwned>(
    value: Value,
    keys: &[&str],
) -> Result<Vec<T>> {
    let array = match value {
        Value::Array(items) => items,
        Value::Object(map) => {
            let found = keys
                .iter()
                .chain(["data", "items", "results"].iter())
                .find_map(|key| map.get(*key));
            match found {
                Some(Value::Array(items)) => items.clone(),
                // An object with no recognizable array key is not an empty list — it is
                // a response shape we do not understand, and rendering it as empty would
                // be a lie. Say so instead.
                _ => bail!(
                    "unexpected list response shape from the control plane (expected an array, or an object with one of: {})",
                    keys.join(", ")
                ),
            }
        }
        other => bail!("unexpected list response from the control plane: {}", other),
    };

    array
        .into_iter()
        .map(|item| serde_json::from_value(item).map_err(anyhow::Error::from))
        .collect()
}

pub(super) fn get_value(auth_mode: &AuthMode, path: &str, missing: Missing) -> Result<Value> {
    let url = managed_url(path);
    let response = get_with_auth(auth_mode, &url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    response
        .json()
        .with_context(|| format!("Failed to parse the response from {}", url))
}

/// Like `get_value`, but reports a missing endpoint as `Ok(None)` instead of an error,
/// so a caller that has a fallback source for the same data can use it.
pub(super) fn get_value_if_supported(auth_mode: &AuthMode, path: &str) -> Result<Option<Value>> {
    let url = managed_url(path);
    let response = get_with_auth(auth_mode, &url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    let response = ensure_success(response, Missing::Endpoint)?;
    let value = response
        .json()
        .with_context(|| format!("Failed to parse the response from {}", url))?;
    Ok(Some(value))
}

pub(super) fn post_json<T: serde::de::DeserializeOwned>(
    auth_mode: &AuthMode,
    path: &str,
    body: Value,
    missing: Missing,
) -> Result<T> {
    let url = managed_url(path);
    let response =
        post_with_auth(auth_mode, &url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    response
        .json()
        .with_context(|| format!("Failed to parse the response from {}", url))
}

/// PATCH.
///
/// Unlike `get_value` / `post_json` this takes no `&AuthMode`, and the asymmetry is
/// deliberate. `get_with_auth` and `post_with_auth` already existed and are shared with
/// commands that really do support HMAC. Nothing here does: `resolve_managed_auth`
/// refuses HMAC before any of these run, so a `patch_with_auth` would carry an
/// unreachable HMAC arm — precisely the dead dispatch that main removed from
/// `http_client` in 817afe8d6. `patch` still attaches the session token and keeps the
/// refresh-and-retry behavior; it just does not branch on a mode that cannot occur.
pub(super) fn patch_json<T: serde::de::DeserializeOwned>(
    path: &str,
    body: Value,
    missing: Missing,
) -> Result<T> {
    let url = managed_url(path);
    let response = patch(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    response
        .json()
        .with_context(|| format!("Failed to parse the response from {}", url))
}

/// PUT, tolerating a response with no body.
///
/// Takes no `&AuthMode` for the same reason as `patch_json` — see its note.
///
/// Returns `Option<Value>` rather than deserializing into a type because the variable
/// upsert routes answer `200` with a bare string (`'Variable set'`), not JSON — and a
/// CLI that failed with "expected value at line 1 column 1" would be reporting a parse
/// error for a request that completely succeeded. Staying tolerant also covers a server
/// that later returns the stored record, or `204` with nothing.
pub(super) fn put_json_optional(
    path: &str,
    body: Value,
    missing: Missing,
) -> Result<Option<Value>> {
    let url = managed_url(path);
    let response = put(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    let text = response
        .text()
        .with_context(|| format!("Failed to read the response from {}", url))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str(&text).ok())
}

/// POST with NO credentials attached.
///
/// Used only by `instance claim`. That endpoint is declared `access: 'public'` on the
/// control plane because the person claiming an instance is the end customer, who has
/// no ForkLaunch account and never will — the one-time token in the body *is* the
/// credential. Sending an operator's session token here would be wrong in both
/// directions: it is not required, and it would make a customer-facing command silently
/// depend on whoever happened to be logged in on that machine.
pub(super) fn post_json_public<T: serde::de::DeserializeOwned>(
    path: &str,
    body: Value,
    missing: Missing,
) -> Result<T> {
    let url = managed_url(path);
    let response =
        post_unauthenticated(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    response
        .json()
        .with_context(|| format!("Failed to parse the response from {}", url))
}

/// POST where the control plane answers with a bare status string rather than JSON
/// (`resume-provisioning` says `Provisioning re-queued`), so the body is returned as
/// text instead of being parsed. Takes no `&AuthMode` for the same reason as
/// `patch_json` — see its note.
pub(super) fn post_text(path: &str, body: Value, missing: Missing) -> Result<String> {
    let url = managed_url(path);
    let response = post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    Ok(body_snippet(response))
}

/// DELETE where the control plane answers `202` with a bare status string rather than
/// JSON, so the body is returned as text instead of being parsed.
/// Takes no `&AuthMode` for the same reason as `patch_json` — see its note.
pub(super) fn delete_text(path: &str, missing: Missing) -> Result<String> {
    let url = managed_url(path);
    let response = delete(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let response = ensure_success(response, missing)?;
    Ok(body_snippet(response))
}

/// Prints the request a `--dryrun` invocation would have made, instead of making it.
pub(super) fn print_dryrun(method: &str, path: &str, body: Option<&Value>) -> Result<()> {
    println!("[DRYRUN] {} {}", method, managed_url(path));
    if let Some(body) = body {
        println!("{}", serde_json::to_string_pretty(body)?);
    }
    println!("[DRYRUN] no request was sent.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::types::AppTemplate;

    #[test]
    fn managed_url_is_rooted_at_the_managed_mode_router() {
        unsafe {
            std::env::set_var(
                "FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL",
                "https://platform.example.com",
            );
        }
        assert_eq!(
            managed_url("/templates"),
            "https://platform.example.com/managed-mode/templates"
        );
        assert_eq!(
            managed_url("/instances/abc/claim-link"),
            "https://platform.example.com/managed-mode/instances/abc/claim-link"
        );
        // `claim` and `claim-link` are different endpoints with different audiences —
        // a path typo here would silently point the customer-facing command at the
        // operator-facing one.
        assert_eq!(
            managed_url("/instances/abc/claim"),
            "https://platform.example.com/managed-mode/instances/abc/claim"
        );
        assert_eq!(
            managed_url("/templates/clinic"),
            "https://platform.example.com/managed-mode/templates/clinic"
        );
        unsafe {
            std::env::remove_var("FORKLAUNCH_PLATFORM_MANAGEMENT_API_URL");
        }
    }

    #[test]
    fn summary_without_available_field_is_treated_as_available() {
        let summary: ManagedModeSummary = serde_json::from_str("{}").unwrap();
        assert!(summary.is_available());
    }

    #[test]
    fn summary_reports_unavailable_with_reason() {
        let summary: ManagedModeSummary = serde_json::from_str(
            r#"{"available":false,"unavailableReason":"Managed apps is not configured for this deployment (MANAGED_APPS_URL is unset).","instances":[],"counts":{"total":0}}"#,
        )
        .unwrap();
        assert!(!summary.is_available());
        assert_eq!(
            summary.unavailable_reason.as_deref(),
            Some("Managed apps is not configured for this deployment (MANAGED_APPS_URL is unset).")
        );
    }

    #[test]
    fn summary_parses_embedded_instances() {
        let summary: ManagedModeSummary = serde_json::from_str(
            r#"{"available":true,"instances":[{"id":"i1","templateSlug":"clinic","host":"a.example.com","region":"us-west-2","state":"active","relayEligible":true}],"relayConfigs":[],"counts":{"total":1,"relayEligible":1,"failed":0}}"#,
        )
        .unwrap();
        assert_eq!(summary.instances.len(), 1);
        assert_eq!(
            summary.instances[0].template_slug.as_deref(),
            Some("clinic")
        );
        assert_eq!(summary.instances[0].state.as_deref(), Some("active"));
    }

    #[test]
    fn extract_list_accepts_a_bare_array() {
        let templates: Vec<AppTemplate> =
            extract_list(serde_json::json!([{"slug":"a","name":"A"}]), &["templates"]).unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].slug.as_deref(), Some("a"));
    }

    #[test]
    fn extract_list_accepts_a_wrapped_array() {
        let templates: Vec<AppTemplate> = extract_list(
            serde_json::json!({"templates":[{"slug":"a"},{"slug":"b"}]}),
            &["templates"],
        )
        .unwrap();
        assert_eq!(templates.len(), 2);
    }

    #[test]
    fn extract_list_accepts_generic_envelope_keys() {
        let templates: Vec<AppTemplate> =
            extract_list(serde_json::json!({"data":[{"slug":"a"}]}), &["templates"]).unwrap();
        assert_eq!(templates.len(), 1);
    }

    #[test]
    fn extract_list_refuses_to_pretend_an_unknown_shape_is_empty() {
        let result: Result<Vec<AppTemplate>> =
            extract_list(serde_json::json!({"unexpected": 1}), &["templates"]);
        assert!(result.is_err());
    }

    #[test]
    fn snippet_prefers_a_json_message_field() {
        assert_eq!(
            snippet_from_text(r#"{"message":"No published template 'clinic'"}"#),
            "No published template 'clinic'"
        );
    }

    #[test]
    fn snippet_unwraps_a_bare_json_string_body() {
        assert_eq!(
            snippet_from_text(r#""Destroy requested""#),
            "Destroy requested"
        );
    }

    #[test]
    fn snippet_falls_back_to_trimmed_text() {
        assert_eq!(
            snippet_from_text("  Cannot POST /managed-mode/templates  "),
            "Cannot POST /managed-mode/templates"
        );
    }

    #[test]
    fn unrouted_404_bodies_are_recognized() {
        assert!(is_unrouted_404("Cannot POST /managed-mode/templates"));
        assert!(is_unrouted_404("Cannot GET /managed-mode/instances"));
        assert!(is_unrouted_404("Cannot DELETE /managed-mode/instances/abc"));
    }

    #[test]
    fn handler_authored_404_bodies_are_not_mistaken_for_missing_routes() {
        assert!(!is_unrouted_404(
            "No claim link available — it was already revealed, expired, or the instance is claimed"
        ));
        assert!(!is_unrouted_404("No published template 'clinic'"));
        // A resource whose name happens to start with the word "Cannot" must not trip it.
        assert!(!is_unrouted_404("Cannot find that instance"));
    }

    #[test]
    fn snippet_truncates_a_flood_of_html() {
        let flood = "x".repeat(5000);
        let snippet = snippet_from_text(&flood);
        assert_eq!(snippet.chars().count(), 401);
        assert!(snippet.ends_with('…'));
    }
}
