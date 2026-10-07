use anyhow::Result;
use reqwest::{
    Method,
    blocking::{Client, Response},
};
use serde_json::Value;
use std::io::{IsTerminal, Write};

use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::hmac::{AuthMode, generate_hmac_auth_header};
use super::token::{force_refresh_token, get_token, get_token_path};

/// Makes an authenticated HTTP request with automatic token refresh and retry logic
///
/// Only a 401 means the credential itself was rejected. On 401 this function
/// refreshes the token (or, if that fails, runs the login flow) and retries
/// once. A 403 is a permission answer about a signed-in user ("you may not do
/// this"), so it is returned to the caller as-is: it used to be treated like
/// a 401, which deleted a perfectly valid login every time a viewer-role user
/// hit a write endpoint.
pub fn make_authenticated_request(
    method: Method,
    url: &str,
    body: Option<Value>,
) -> Result<Response> {
    match try_authenticated_request(method.clone(), url, body.clone(), false) {
        Ok(response) => {
            if response.status() == 401 {
                handle_auth_failure_and_retry(method, url, body)
            } else {
                Ok(response)
            }
        }
        Err(e) => Err(e),
    }
}

/// Attempts to make an authenticated request
fn try_authenticated_request(
    method: Method,
    url: &str,
    body: Option<Value>,
    force_refresh: bool,
) -> Result<Response> {
    let token = if force_refresh {
        let token_path = get_token_path()?;
        if token_path.exists() {
            std::fs::remove_file(&token_path)?;
        }
        get_token()?
    } else {
        get_token()?
    };

    let client = Client::new();
    let mut request = client
        .request(method, url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/json")
        .header("User-Agent", user_agent());

    if let Some(json_body) = body {
        request = request.json(&json_body);
    }

    Ok(request.send()?)
}

/// Identifies this CLI as the caller in platform request logs — lets the platform
/// distinguish CLI-originated mutations (e.g. `fl infra resize`) from dashboard-UI
/// or other callers of the same endpoints.
fn user_agent() -> String {
    format!("forklaunch-cli/{}", env!("CARGO_PKG_VERSION"))
}

/// Handles authentication failure by refreshing token or triggering re-login
fn handle_auth_failure_and_retry(
    method: Method,
    url: &str,
    body: Option<Value>,
) -> Result<Response> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    // The server rejected the access token. Try the silent refresh path
    // first; only if that still fails do we discard the stored login and
    // ask the user.
    match force_refresh_token() {
        Ok(_) => try_authenticated_request(method, url, body, false),
        Err(_) => {
            let token_path = get_token_path()?;
            if token_path.exists() {
                std::fs::remove_file(&token_path)?;
            }
            log_warn!(stdout, "\nAuthentication expired. Please log in again.");

            // No terminal means nobody can complete the device flow; a
            // clear error beats a login prompt that hangs a CI job.
            if !std::io::stdin().is_terminal() {
                anyhow::bail!(
                    "Authentication expired and no terminal is attached. Run `forklaunch login` (or `forklaunch login --token <token>`) and retry"
                );
            }
            crate::login::login()?;
            try_authenticated_request(method, url, body, false)
        }
    }
}

/// Helper to make a GET request with authentication
pub fn get(url: &str) -> Result<Response> {
    make_authenticated_request(Method::GET, url, None)
}

/// Helper to make a POST request with authentication
pub fn post(url: &str, body: Value) -> Result<Response> {
    make_authenticated_request(Method::POST, url, Some(body))
}

/// Helper to make a PUT request with authentication
pub fn put(url: &str, body: Value) -> Result<Response> {
    make_authenticated_request(Method::PUT, url, Some(body))
}

/// Helper to make a PATCH request with authentication
pub fn patch(url: &str, body: Value) -> Result<Response> {
    make_authenticated_request(Method::PATCH, url, Some(body))
}

/// Helper to make a DELETE request with authentication
pub fn delete(url: &str) -> Result<Response> {
    make_authenticated_request(Method::DELETE, url, None)
}

/// POST with NO credentials and NO token lookup.
///
/// Every other helper here funnels through `make_authenticated_request`, which calls
/// `get_token()` and, if that fails, drops the user into an interactive login. That is
/// the wrong behavior for a route the platform declares `access: 'public'` and that a
/// person with no ForkLaunch account is expected to call — `managed instance claim` is
/// the case this exists for, where a one-time token in the body is the credential.
///
/// Keeps the `Accept` and `User-Agent` headers so these requests are still identifiable
/// as CLI-originated in platform logs.
pub fn post_unauthenticated(url: &str, body: Value) -> Result<Response> {
    let client = Client::new();
    Ok(client
        .post(url)
        .header("Accept", "application/json")
        .header("User-Agent", user_agent())
        .json(&body)
        .send()?)
}

/// Extract the path component from a full URL (e.g. "https://host:port/path?q" -> "/path?q")
fn extract_url_path(url: &str) -> Result<String> {
    // Find the start of the path after scheme://host(:port)
    let after_scheme = url
        .find("://")
        .map(|i| i + 3)
        .ok_or_else(|| anyhow::anyhow!("Invalid URL: no scheme found in '{}'", url))?;
    let path_start = url[after_scheme..]
        .find('/')
        .map(|i| after_scheme + i)
        .unwrap_or(url.len());
    if path_start >= url.len() {
        return Ok("/".to_string());
    }
    Ok(url[path_start..].to_string())
}

/// Makes an HMAC-authenticated HTTP request. No retry/re-login logic since HMAC secrets are static.
fn make_hmac_request(
    secret_key: &str,
    method: Method,
    url: &str,
    body: Option<Value>,
) -> Result<Response> {
    let path = extract_url_path(url)?;
    make_hmac_request_with_sign_path(secret_key, method, url, &path, body)
}

/// Same as `make_hmac_request` but lets the caller specify the path that gets
/// signed. Use when the backend's HMAC verifier uses a router-relative path
/// that differs from the full URL path (e.g. forklaunch's `@forklaunch/core`
/// uses `req.path`, which strips the router base mount).
fn make_hmac_request_with_sign_path(
    secret_key: &str,
    method: Method,
    url: &str,
    sign_path: &str,
    body: Option<Value>,
) -> Result<Response> {
    let auth_header = generate_hmac_auth_header(
        secret_key,
        method.as_str(),
        sign_path,
        body.as_ref(),
    )?;

    let client = Client::new();
    let mut request = client
        .request(method, url)
        .header("Authorization", auth_header)
        .header("Accept", "application/json")
        .header("User-Agent", user_agent());

    if let Some(json_body) = body {
        request = request.json(&json_body);
    }

    Ok(request.send()?)
}

/// POST with auth mode dispatch (JWT or HMAC)
pub fn post_with_auth(auth_mode: &AuthMode, url: &str, body: Value) -> Result<Response> {
    match auth_mode {
        AuthMode::Jwt => post(url, body),
        AuthMode::Hmac { secret_key } => {
            make_hmac_request(secret_key, Method::POST, url, Some(body))
        }
    }
}

/// POST with auth mode dispatch, where the HMAC sign path is supplied by the
/// caller (use when the backend's `req.path` differs from the URL path).
pub fn post_with_auth_and_sign_path(
    auth_mode: &AuthMode,
    url: &str,
    sign_path: &str,
    body: Value,
) -> Result<Response> {
    match auth_mode {
        AuthMode::Jwt => post(url, body),
        AuthMode::Hmac { secret_key } => make_hmac_request_with_sign_path(
            secret_key,
            Method::POST,
            url,
            sign_path,
            Some(body),
        ),
    }
}

/// GET with auth mode dispatch (JWT or HMAC)
pub fn get_with_auth(auth_mode: &AuthMode, url: &str) -> Result<Response> {
    match auth_mode {
        AuthMode::Jwt => get(url),
        AuthMode::Hmac { secret_key } => make_hmac_request(secret_key, Method::GET, url, None),
    }
}

/// PUT with auth mode dispatch (JWT or HMAC)
pub fn put_with_auth(auth_mode: &AuthMode, url: &str, body: Value) -> Result<Response> {
    match auth_mode {
        AuthMode::Jwt => put(url, body),
        AuthMode::Hmac { secret_key } => {
            make_hmac_request(secret_key, Method::PUT, url, Some(body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url_path_basic() {
        assert_eq!(
            extract_url_path("https://api.example.com/releases").unwrap(),
            "/releases"
        );
    }

    #[test]
    fn test_extract_url_path_with_port() {
        assert_eq!(
            extract_url_path("https://api.example.com:8080/releases/internal").unwrap(),
            "/releases/internal"
        );
    }

    #[test]
    fn test_extract_url_path_with_query() {
        assert_eq!(
            extract_url_path("https://api.example.com/deployments/123?status=true").unwrap(),
            "/deployments/123?status=true"
        );
    }

    #[test]
    fn test_extract_url_path_no_path() {
        assert_eq!(
            extract_url_path("https://api.example.com").unwrap(),
            "/"
        );
    }

    #[test]
    fn test_extract_url_path_root() {
        assert_eq!(
            extract_url_path("https://api.example.com/").unwrap(),
            "/"
        );
    }
}
