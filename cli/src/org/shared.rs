use anyhow::{Context, Result, bail};
use reqwest::{Method, blocking::Response};

use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_iam_api_url},
    core::{http_client, http_client::make_authenticated_request, validate::require_auth},
};

/// One place for the "call IAM as the signed-in user" shape every org
/// subcommand needs, so each command file is just its arguments and output.
pub(super) fn iam_url(path: &str) -> String {
    format!("{}{}", get_iam_api_url(), path)
}

pub(super) fn iam_get(path: &str) -> Result<serde_json::Value> {
    let _token = require_auth()?;
    let response =
        http_client::get(&iam_url(path)).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    ensure_ok(response, "read")?
        .json()
        .with_context(|| "Failed to parse the response from IAM")
}

pub(super) fn iam_send(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
    action: &str,
) -> Result<Response> {
    let _token = require_auth()?;
    let response = make_authenticated_request(method, &iam_url(path), body)
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    ensure_ok(response, action)
}

fn ensure_ok(response: Response, action: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let detail = response.text().unwrap_or_default();
    // 403 is the common one here: these routes are admin-only, and a member
    // who runs them should be told that rather than "request failed".
    if status.as_u16() == 403 {
        bail!(
            "You do not have permission to {} this ({}). Organization administration needs the admin role.",
            action,
            status
        );
    }
    bail!("Failed to {} ({}): {}", action, status, detail)
}
