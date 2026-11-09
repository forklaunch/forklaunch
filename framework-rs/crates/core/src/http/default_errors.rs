//! Explicit finite-body TypeScript default error-string compatibility.
use crate::http::{Body, FrameworkResponse, Headers, typescript_parser_error_response};
use forklaunch_core::contract::{
    BodyContract, BodyKind, ContractRegistry, ResponseContract, RouteContract, SchemaRef,
    ValidationMode,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

#[derive(Clone)]
struct Registration {
    responses: Vec<ResponseContract>,
    versions: Vec<forklaunch_core::contract::VersionedContract>,
    statuses: BTreeSet<u16>,
    max_bytes: usize,
    timeout: Duration,
}

/// Opt-in missing default error statuses for explicitly registered routes. Only selected
/// statuses are added; explicit declarations and unselected statuses remain
/// unchanged. In error-validation mode finite plain text, JSON-encoded strings
/// and empty responses are accepted. JSON strings become plain text, matching
/// the TypeScript enriched sender. Other values are rejected without payloads;
/// primitive-type diagnostics match the actual Zod v3 default string union.
/// Bounds apply only to selected response bodies; no success body is buffered.
/// Existing response validators still run, including caller-owned header checks.
/// Local execution normalizes handler-returned responses. Axum also applies
/// selected-status normalization after authentication/parser failures and handler
/// error conversion at the middleware boundary, and rejects
/// known incompatible parser/authentication configurations. Unselected statuses
/// remain untouched; direct local handler failures still return `Err`.
/// Zero-byte bodies are admitted as no-body, preserving headers except plain-text
/// media-type normalization: an Axum transport cannot distinguish an empty byte
/// buffer from an absent body.
/// This is not parity with TypeScript's original-value checks on empty buffers.
/// This does not implement arbitrary JavaScript values or streaming errors.
#[derive(Clone, Default)]
pub struct DefaultErrorResponses {
    entries: BTreeMap<String, Registration>,
}
impl DefaultErrorResponses {
    /// Register missing default statuses and positive body collection bounds.
    /// # Errors
    /// Rejects existing/duplicate statuses, non-default statuses, versioned routes,
    /// duplicate operations, empty selections and zero bounds.
    pub fn insert(
        &mut self,
        route: &RouteContract,
        statuses: &[u16],
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<(), String> {
        let selected: BTreeSet<_> = statuses.iter().copied().collect();
        if !route.versions.is_empty()
            || selected.is_empty()
            || selected.len() != statuses.len()
            || selected.iter().any(|status| {
                ![400, 401, 403, 404, 500].contains(status)
                    || route
                        .responses
                        .iter()
                        .any(|response| response.status_code == *status)
            })
            || max_bytes == 0
            || timeout.is_zero()
        {
            return Err("Default error profiles require missing default statuses, unversioned routes and positive bounds".into());
        }
        let key = key(route);
        if self.entries.contains_key(&key) {
            return Err("Duplicate default error profile operation".into());
        }
        self.entries.insert(
            key,
            Registration {
                responses: route.responses.clone(),
                versions: Vec::new(),
                statuses: selected,
                max_bytes,
                timeout,
            },
        );
        Ok(())
    }

    /// Register missing default strings in every version, with finite collection bounds.
    /// Requires selected 500 so normalization failures also have a truthful text declaration.
    /// Request validation must be Error: unmatched requests in None/Warning modes
    /// can trigger TypeScript final-error redaction instead of the handler body.
    /// Original version schemas remain available to caller-owned response validators.
    /// No request-selection diagnostic equivalence or explicit-schema unions are implied.
    /// # Errors
    /// Rejects unversioned routes, selected 400, missing 500, any status already declared
    /// at the root or in any version, duplicate operations and nonpositive bounds.
    pub fn insert_versioned(
        &mut self,
        route: &RouteContract,
        statuses: &[u16],
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<(), String> {
        if route.options.request_validation != ValidationMode::Error
            || route.versions.is_empty()
            || statuses.contains(&400)
            || !statuses.contains(&500)
            || route.versions.values().any(|version| {
                version
                    .responses
                    .iter()
                    .any(|response| statuses.contains(&response.status_code))
            })
        {
            return Err("Versioned default errors require missing statuses in every version, selected 500, request Error validation and no selected 400".into());
        }
        let mut unversioned = route.clone();
        unversioned.versions.clear();
        self.insert(&unversioned, statuses, max_bytes, timeout)?;
        self.entries
            .get_mut(&key(route))
            .ok_or_else(|| "Missing default error registration".to_owned())?
            .versions = route.versions.values().cloned().collect();
        Ok(())
    }

    /// Verify profile registrations against original route declarations.
    /// # Errors
    /// Rejects missing/ambiguous routes or changed response/version declarations.
    pub fn validate_routes(&self, routes: &[RouteContract]) -> Result<(), String> {
        for (key, entry) in &self.entries {
            let matches: Vec<_> = routes
                .iter()
                .filter(|route| self::key(route) == *key)
                .collect();
            if matches.len() != 1
                || !versions_match(matches[0], entry)
                || matches[0].responses != entry.responses
            {
                return Err(
                    "Default error profile no longer matches exactly one original route".into(),
                );
            }
        }
        Ok(())
    }

    /// Verify the original declaration for one local route, when registered.
    /// # Errors
    /// Rejects stale response or version registrations.
    pub fn validate_route(&self, route: &RouteContract) -> Result<(), String> {
        if let Some(entry) = self.entries.get(&key(route))
            && (!versions_match(route, entry) || route.responses != entry.responses)
        {
            return Err("Default error profile no longer matches its route".into());
        }
        Ok(())
    }

    /// Check native request failure profiles before executing a registered route.
    /// # Errors
    /// Rejects stale declarations and known JSON parser/request errors that would
    /// contradict selected text error statuses.
    pub fn validate_execution(
        &self,
        route: &RouteContract,
        parser_format: crate::http::ParserErrorFormat,
    ) -> Result<(), String> {
        self.validate_route(route)?;
        if self.limits(route, 400).is_some()
            && (parser_format != crate::http::ParserErrorFormat::TypeScriptText
                || !safe_request_presence(route))
        {
            return Err("Selected 400 requires the TypeScript parser profile and no incompatible native request diagnostics".into());
        }
        if self.limits(route, 500).is_some()
            && (route.request.body.is_some()
                || route
                    .versions
                    .values()
                    .any(|version| version.request.body.is_some()))
            && parser_format != crate::http::ParserErrorFormat::TypeScriptText
        {
            return Err(
                "Selected 500 with a request body requires the TypeScript parser profile".into(),
            );
        }
        Ok(())
    }

    /// Add declarations for routes currently present in a staged registry.
    /// Call `validate_routes` against original declarations before exposing the
    /// finished router or exporting this projection. Missing routes are tolerated
    /// here so configuration can precede route registration.
    /// # Errors
    /// Rejects conflicting or ambiguous declarations.
    pub fn project(&self, registry: &ContractRegistry) -> Result<Option<ContractRegistry>, String> {
        if self.entries.is_empty() {
            return Ok(None);
        }
        let mut projected = registry.clone();
        for route in registry.routes() {
            if let Some(entry) = self.entries.get(&key(route)) {
                if entry
                    .responses
                    .iter()
                    .any(|response| !route.responses.contains(response))
                {
                    return Err("Default error profile declarations changed".into());
                }
                if !versions_match(route, entry) {
                    return Err("Default error profile versions changed".into());
                }
                if route.options.response_validation != ValidationMode::Error {
                    continue;
                }
                for status in &entry.statuses {
                    // A declaration contributed by another middleware must not be overwritten.
                    if route
                        .responses
                        .iter()
                        .any(|response| response.status_code == *status)
                    {
                        return Err(
                            "Default error profile conflicts with an existing response".into()
                        );
                    }
                    if entry.versions.is_empty() {
                        projected.declare_response(
                            route.method,
                            &route.operation_name,
                            declaration(*status),
                        )?;
                    } else {
                        for version in &entry.versions {
                            projected.declare_version_response(
                                route.method,
                                &route.operation_name,
                                &version.version,
                                declaration(*status),
                            )?;
                        }
                    }
                }
            }
        }
        Ok(Some(projected))
    }

    /// Merge registrations while composing routers.
    /// # Errors
    /// Rejects duplicate operations without changing either registration.
    pub fn extend(&mut self, other: Self) -> Result<(), String> {
        if other
            .entries
            .keys()
            .any(|key| self.entries.contains_key(key))
        {
            return Err("Duplicate default error profile operation".into());
        }
        self.entries.extend(other.entries);
        Ok(())
    }

    /// Collection bounds for an active selected status. Other responses are untouched.
    #[must_use]
    pub fn limits(&self, route: &RouteContract, status: u16) -> Option<(usize, Duration)> {
        if self.entries.is_empty() || route.options.response_validation != ValidationMode::Error {
            return None;
        }
        self.entries
            .get(&key(route))
            .filter(|entry| entry.statuses.contains(&status))
            .map(|entry| (entry.max_bytes, entry.timeout))
    }

    /// Normalize only registered error responses, enforcing their finite string body.
    /// # Errors
    /// Returns a safe response on invalid media/encoding/body/size. No payload is disclosed.
    pub fn normalize(
        &self,
        route: &RouteContract,
        mut response: FrameworkResponse,
        correlation_id: &str,
    ) -> Result<FrameworkResponse, FrameworkResponse> {
        let Some((max_bytes, _)) = self.limits(route, response.status_code) else {
            return Ok(response);
        };
        if response.body.is_empty()
            || matches!(&response.body, Body::Text(value) if value.is_empty())
            || matches!(&response.body, Body::Binary(value) if value.is_empty())
        {
            if header(&response.headers, "content-type").is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("text/plain")
            }) {
                response
                    .headers
                    .retain(|name, _| !name.eq_ignore_ascii_case("content-type"));
                response
                    .headers
                    .insert("content-type".into(), "text/plain; charset=utf-8".into());
            }
            return Ok(response);
        }
        let versioned = self
            .entries
            .get(&key(route))
            .is_some_and(|entry| !entry.versions.is_empty());
        let text = match &response.body {
            Body::Empty => return Ok(response),
            Body::Binary(bytes) => {
                return Err(if bytes.len() > max_bytes {
                    safe_failure(correlation_id)
                } else {
                    invalid_type("object", correlation_id, versioned)
                });
            }
            Body::Text(text) => text,
        };
        if text.len() > max_bytes
            || header(&response.headers, "content-encoding")
                .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
        {
            return Err(safe_failure(correlation_id));
        }
        let media = header(&response.headers, "content-type")
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let normalized = if media == "application/json"
            || media.starts_with("application/") && media.ends_with("+json")
        {
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(serde_json::Value::String(value)) => value,
                Ok(value) => {
                    return Err(invalid_type(
                        match value {
                            serde_json::Value::Null => "null",
                            serde_json::Value::Bool(_) => "boolean",
                            serde_json::Value::Number(_) => "number",
                            serde_json::Value::Array(_) => "array",
                            serde_json::Value::Object(_) => "object",
                            serde_json::Value::String(_) => unreachable!(),
                        },
                        correlation_id,
                        versioned,
                    ));
                }
                Err(_) => return Err(safe_failure(correlation_id)),
            }
        } else if media == "text/plain" {
            text.clone()
        } else {
            return Err(safe_failure(correlation_id));
        };
        response.body = Body::Text(normalized);
        clear_representation_headers(&mut response.headers);
        response
            .headers
            .insert("content-type".into(), "text/plain; charset=utf-8".into());
        Ok(response)
    }
}

fn declaration(status: u16) -> ResponseContract {
    ResponseContract::new(
        status,
        Some(BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String"),
            content_type: Some("text/plain".into()),
        }),
    )
}
fn key(route: &RouteContract) -> String {
    format!("{} {}", route.method.as_str(), route.operation_name)
}
fn header<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}
fn clear_representation_headers(headers: &mut Headers) {
    headers.retain(|name, _| {
        ![
            "content-type",
            "content-length",
            "content-encoding",
            "content-range",
            "transfer-encoding",
            "etag",
            "digest",
            "content-digest",
            "repr-digest",
            "content-md5",
            "trailer",
            "last-modified",
        ]
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
    });
}
fn safe_failure(correlation_id: &str) -> FrameworkResponse {
    typescript_parser_error_response(500, correlation_id)
}
fn invalid_type(received: &str, correlation_id: &str, versioned: bool) -> FrameworkResponse {
    let second = if versioned {
        "Invalid input".to_owned()
    } else {
        format!("Expected string, received {received}")
    };
    FrameworkResponse::text(500, format!("Invalid response:\nResponse validation failed with the following errors:\n1. Path: Union Schema Variant 0\n   Message: Expected string, received {received}\n\n2. Path: Union Schema Variant 1\n   Message: {second}\n\nCorrelation id: {correlation_id}"))
        .with_header("content-type", "text/plain; charset=utf-8")
}

#[cfg(test)]
#[path = "default_errors_tests.rs"]
mod tests;

fn safe_request_presence(route: &RouteContract) -> bool {
    if route.options.request_validation != ValidationMode::Error {
        return true;
    }
    route.request.body.is_none()
        && route.request.query.is_empty()
        && route.request.headers.is_empty()
        && route.request.params.iter().all(|field| {
            field.schema.name == "String"
                && field.schema.json_schema.is_none()
                && [&route.path, &route.openapi_path].iter().all(|path| {
                    path.split('/').any(|part| {
                        part == format!(":{}", field.name) || part == format!("{{{}}}", field.name)
                    })
                })
        })
}

fn versions_match(route: &RouteContract, entry: &Registration) -> bool {
    (entry.versions.is_empty() || route.options.request_validation == ValidationMode::Error)
        && route.versions.values().eq(entry.versions.iter())
}
