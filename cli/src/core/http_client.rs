use anyhow::Result;
use reqwest::{
    Method,
    blocking::{Client, Response},
};
use serde_json::Value;
use std::io::Write;

use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::hmac::{AuthMode, generate_hmac_auth_header};
use super::token::{get_token, get_token_path};

/// Makes an authenticated HTTP request with automatic token refresh and retry logic
///
/// If the request returns 401 or 403, this function will:
/// 1. Force token refresh by calling get_token() again
/// 2. Retry the request once with the new token
/// 3. If refresh fails, trigger auto re-login flow
pub fn make_authenticated_request(
    method: Method,
    url: &str,
    body: Option<Value>,
) -> Result<Response> {
    match try_authenticated_request(method.clone(), url, body.clone(), false) {
        Ok(response) => {
            let status = response.status();

            if status == 401 || status == 403 {
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
        .header("Accept", "application/json");

    if let Some(json_body) = body {
        request = request.json(&json_body);
    }

    Ok(request.send()?)
}

/// Handles authentication failure by refreshing token or triggering re-login
fn handle_auth_failure_and_retry(
    method: Method,
    url: &str,
    body: Option<Value>,
) -> Result<Response> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    // Try to refresh the token first by deleting the token file
    // This will trigger a fresh token fetch on next get_token() call
    let token_path = get_token_path()?;
    if token_path.exists() {
        std::fs::remove_file(&token_path)?;
    }

    match get_token() {
        Ok(_) => {
            try_authenticated_request(method, url, body, false)
        }
        Err(_) => {
            log_warn!(stdout, "\nAuthentication expired. Please log in again.");

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

/// Helper to make a DELETE request with authentication
#[allow(dead_code)]
pub fn delete(url: &str) -> Result<Response> {
    make_authenticated_request(Method::DELETE, url, None)
}

/// Helper to make a PATCH request with authentication
#[allow(dead_code)]
pub fn patch(url: &str, body: Value) -> Result<Response> {
    make_authenticated_request(Method::PATCH, url, Some(body))
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
    let auth_header = generate_hmac_auth_header(
        secret_key,
        method.as_str(),
        &path,
        body.as_ref(),
    )?;

    let client = Client::new();
    let mut request = client
        .request(method, url)
        .header("Authorization", auth_header)
        .header("Accept", "application/json");

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
