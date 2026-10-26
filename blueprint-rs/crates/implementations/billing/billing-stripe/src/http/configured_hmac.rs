//! Actual configured billing HMAC header semantics. Legacy authentication remains separate.
//!
//! Signed path: the full wire path (`POST /payment-link`), matching the framework
//! verifier, every other module and the generated Rust/TypeScript SDK signers.
//! The Express router-relative path (`/` for `POST /payment-link`) that older
//! TypeScript callers signed is deprecated but still accepted by default so they
//! keep working. Turn it off with `BillingBackend::with_mount_relative_hmac_paths(false)`
//! or `HMAC_ACCEPT_MOUNT_RELATIVE_PATHS=false`, because it is not bound to a mount.
use forklaunch_axum::{
    module_auth::{HmacJwksAuth, HmacPayloadError},
    session::AuthenticatedSession,
};
use forklaunch_common::javascript_date::parse_host;
use forklaunch_core::contract::{
    AuthConfig, BodyContract, BodyKind, ResponseContract, RouteContract, SchemaRef,
};
use forklaunch_core::http::{Body, ErrorKind, ErrorResponse, FrameworkRequest};

fn mount(route: &RouteContract) -> Option<&'static str> {
    if !matches!(route.auth, Some(AuthConfig::Hmac(_))) {
        return None;
    }
    [
        "/payment-link",
        "/checkout-session",
        "/subscription",
        "/plan",
    ]
    .into_iter()
    .find(|prefix| route.path == *prefix || route.path.starts_with(&format!("{prefix}/")))
}
pub(super) fn selected(route: &RouteContract) -> bool {
    mount(route).is_some()
}

/// Declare the configured internal routes' plain-text authentication failures.
/// Portal JWT and bound capability routes retain their original declarations.
pub fn project_configured_billing_hmac(route: &mut RouteContract) {
    if !selected(route) {
        return;
    }
    for status in [401, 403] {
        let body = BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String").with_json_schema(r#"{"type":"string"}"#),
            content_type: Some("text/plain".into()),
        };
        if let Some(response) = route
            .responses
            .iter_mut()
            .find(|response| response.status_code == status)
        {
            response.body = Some(body);
        } else {
            route
                .responses
                .push(ResponseContract::new(status, Some(body)));
        }
    }
}
fn format_error() -> ErrorResponse {
    ErrorResponse::new(
        ErrorKind::Unauthorized,
        "Invalid Authorization token format.",
    )
}
fn internal(request: &FrameworkRequest) -> ErrorResponse {
    let response = forklaunch_core::http::typescript_parser_error_response(
        500,
        &request.context.correlation_id,
    );
    ErrorResponse::new(
        ErrorKind::Internal,
        response.body.as_text().unwrap_or("Internal server error"),
    )
}
fn part<'a>(value: &'a str, label: &str) -> Option<&'a str> {
    let (key, value) = value.split_once('=')?;
    (key == label && !value.is_empty()).then_some(value)
}
/// `HMAC_ACCEPT_MOUNT_RELATIVE_PATHS`: `false`/`0` rejects the deprecated
/// router-relative form, `true`/`1` accepts it; unset leaves the default (accept).
pub(super) fn mount_relative_from_env() -> Option<bool> {
    let value = std::env::var("HMAC_ACCEPT_MOUNT_RELATIVE_PATHS").ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "false" | "0" => Some(false),
        "true" | "1" => Some(true),
        _ => None,
    }
}
pub(super) fn authenticate(
    auth: &HmacJwksAuth,
    request: &FrameworkRequest,
    route: &RouteContract,
    accept_mount_relative: bool,
) -> Result<AuthenticatedSession, ErrorResponse> {
    let token = request.headers.get("authorization").ok_or_else(|| {
        ErrorResponse::new(ErrorKind::Unauthorized, "Authorization token required.")
    })?;
    let mut pieces = token.split(' ');
    let prefix = pieces.next().unwrap_or_default();
    let first = pieces.next().ok_or_else(format_error)?;
    if prefix.is_empty() {
        return Err(format_error());
    }
    let rest = [Some(first), pieces.next(), pieces.next(), pieces.next()];
    if prefix != "HMAC" || rest.iter().any(Option::is_none) {
        return Err(ErrorResponse::new(
            ErrorKind::Forbidden,
            "Invalid Authorization token.",
        ));
    }
    let values = rest
        .into_iter()
        .zip(["keyId", "ts", "nonce", "signature"])
        .map(|(value, name)| part(value.unwrap_or_default(), name).ok_or_else(format_error))
        .collect::<Result<Vec<_>, _>>()?;
    let millis = parse_host(values[1]).map_err(|_| internal(request))?;
    let timestamp = iso_timestamp(millis);
    let wire_path = request.raw_path.as_deref().unwrap_or(&request.path);
    let body = match &request.body {
        Body::Empty => "undefined".into(),
        Body::Text(value) => format!("{value}\n"),
        Body::Binary(value) => format!("{}\n", String::from_utf8_lossy(value)),
    };
    let payload = |path: &str| {
        format!(
            "{}\n{path}\n{body}{timestamp}\n{}",
            request.method.as_str(),
            values[2]
        )
    };
    // The signed path is the full wire path, as every other module and both
    // generated SDK signers use. The deprecated Express router-relative form is a fallback.
    let mut candidates = vec![wire_path];
    if accept_mount_relative
        && let Some(relative) = mount_relative(wire_path, mount(route).unwrap_or_default())
    {
        candidates.push(relative);
    }
    let mut result = Err(HmacPayloadError::InvalidSignature);
    for path in candidates {
        result = auth.verify_hmac_payload_for_route(
            route,
            values[0],
            payload(path).as_bytes(),
            values[3],
        );
        if !matches!(result, Err(HmacPayloadError::InvalidSignature)) {
            break;
        }
    }
    result.map_err(|error| match error {
        HmacPayloadError::InvalidSignature => {
            ErrorResponse::new(ErrorKind::Forbidden, "Invalid Authorization signature.")
        }
        HmacPayloadError::NotHmac | HmacPayloadError::UnknownKey => internal(request),
    })
}
/// The Express router-relative form of a mounted path (`/payment-link/x` → `/x`).
fn mount_relative<'a>(wire_path: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty()
        || !wire_path
            .get(..prefix.len())
            .is_some_and(|part| part.eq_ignore_ascii_case(prefix))
    {
        return None;
    }
    let path = &wire_path[prefix.len()..];
    Some(if path.is_empty() { "/" } else { path })
}
// Civil-date conversion over the complete JavaScript TimeClip range, including
// signed extended years, without chrono's narrower representable year range.
fn iso_timestamp(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let within = millis.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+07}")
    };
    format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000
    )
}

#[cfg(test)]
mod tests;
