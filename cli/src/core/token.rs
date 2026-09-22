use std::{
    env::var,
    fs::{read_to_string, remove_file, write},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::constants::get_iam_api_url;

/// The prefix better-auth stamps on machine credentials
/// (`apiKey({ defaultPrefix: 'flk_' })` in the platform's auth config).
///
/// It is what lets one `--token` flag take either kind of credential: a JWT
/// from the device flow is opaque base64url and never starts like this, so a
/// value carrying this prefix is unambiguously an API key that has to be
/// exchanged for a JWT before it can reach a platform route.
pub(crate) const API_KEY_PREFIX: &str = "flk_";

/// Exchange a long-lived API key for a short-lived JWT.
///
/// The platform enables `enableSessionForAPIKeys`, so presenting the key at
/// the token endpoint mints a session and returns a JWT the same way the
/// browser flow does. That indirection is the whole reason a key cannot
/// simply be stored as an access token: the framework's route guards accept
/// JWTs, not keys.
pub(crate) fn exchange_api_key(api_key: &str) -> Result<(String, i64)> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/api/auth/token", get_iam_api_url()))
        .header("x-api-key", api_key)
        .header("Accept", "application/json")
        .send()?;

    let status = response.status();
    if !status.is_success() {
        // 401 here means the key is wrong, revoked, or belongs to another
        // environment — worth saying, because the alternative is a confusing
        // failure on the next command instead of this one.
        bail!(
            "The API key was refused ({}). Check that it is current, and that it \
             belongs to the environment this CLI points at.",
            status
        );
    }

    #[derive(Deserialize)]
    struct JwtTokenResponse {
        token: String,
        #[serde(rename = "expiresIn")]
        expires_in: i64,
    }

    let jwt: JwtTokenResponse = response.json()?;
    Ok((jwt.token, chrono::Utc::now().timestamp() + jwt.expires_in))
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenData {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

pub(crate) fn get_token_path() -> Result<PathBuf> {
    Ok(Path::new(&var("HOME")?).join(".forklaunch").join("token"))
}

fn is_token_expired(expires_at: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    // Consider token expired if it expires in less than 60 seconds
    expires_at <= now + 60
}

fn refresh_token(current_token: &str) -> Result<TokenData> {
    // A stored API key renews without anyone present: re-exchange it for a
    // fresh JWT and keep the key as the thing that renews next time. This is
    // what lets an unattended agent keep working past the JWT's lifetime,
    // where a device-flow session would eventually need a person at a browser.
    if current_token.starts_with(API_KEY_PREFIX) {
        let (access_token, expires_at) = exchange_api_key(current_token)?;
        return Ok(TokenData {
            access_token,
            refresh_token: current_token.to_string(),
            expires_at,
        });
    }

    let api_url = get_iam_api_url();
    let client = reqwest::blocking::Client::new();

    // Re-exchange the current access token (which is a session token) for a fresh JWT
    let response = client
        .get(format!("{}/api/auth/token", api_url))
        .header(
            "Cookie",
            format!("better-auth.session_token={}", current_token),
        )
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .send()?;

    if !response.status().is_success() {
        bail!("Failed to refresh token: {}", response.status());
    }

    #[derive(Deserialize)]
    struct JwtTokenResponse {
        token: String,
        #[serde(rename = "refreshToken")]
        refresh_token: Option<String>,
        #[serde(rename = "expiresIn")]
        expires_in: i64,
    }

    let jwt_data: JwtTokenResponse = response.json()?;
    let expires_at = chrono::Utc::now().timestamp() + jwt_data.expires_in;

    // Preserve the original session token if the server doesn't return a new refresh token.
    // Without this, the refresh token becomes empty and subsequent refreshes fail silently,
    // causing authenticated users to appear as having no subscription.
    let refresh_token = jwt_data
        .refresh_token
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| current_token.to_string());

    Ok(TokenData {
        access_token: jwt_data.token,
        refresh_token,
        expires_at,
    })
}

fn save_token_data(token_data: &TokenData) -> Result<()> {
    let token_path = get_token_path()?;
    let toml_content = toml::to_string(token_data)?;
    write(&token_path, toml_content)?;
    Ok(())
}

/// Refresh the stored access token now, regardless of the recorded expiry.
/// Used when the server has just rejected the token with a 401: the stored
/// `expires_at` can be wrong (API-token logins used to record "never"), so
/// the server's answer, not the file, is what says the token is stale.
pub(crate) fn force_refresh_token() -> anyhow::Result<String> {
    let token_path = get_token_path()?;
    let toml_content = read_to_string(&token_path)?;
    let token_data: TokenData = toml::from_str(&toml_content)?;
    if token_data.refresh_token.is_empty() {
        bail!("No refresh token stored");
    }
    let new_token_data = refresh_token(&token_data.refresh_token)?;
    save_token_data(&new_token_data)?;
    Ok(new_token_data.access_token)
}

/// The `exp` claim of a JWT, without verifying it. Used only to decide when
/// to refresh a token the user pasted in; the server still verifies it.
pub(crate) fn jwt_expiry(token: &str) -> Option<i64> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_i64()
}

pub(crate) fn get_token() -> anyhow::Result<String> {
    let token_path = get_token_path()?;

    if !token_path.exists() {
        bail!("No token found. Please run `forklaunch login` to authenticate");
    }

    let toml_content = read_to_string(&token_path)?;
    let mut token_data: TokenData = toml::from_str(&toml_content).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse token file: {}. Please run `forklaunch login` again",
            e
        )
    })?;

    if is_token_expired(token_data.expires_at) {
        // API-token logins carry no refresh token; say so instead of
        // failing a refresh that was never going to work.
        if token_data.refresh_token.is_empty() {
            let _ = remove_file(&token_path);
            bail!(
                "Your API token expired. Generate a new one in the dashboard and run `forklaunch login --token <token>`"
            );
        }
        // Try to refresh the token using the refresh token (session token)
        match refresh_token(&token_data.refresh_token) {
            Ok(new_token_data) => {
                save_token_data(&new_token_data)?;
                token_data = new_token_data;
            }
            Err(_) => {
                // Refresh failed - delete token file and prompt user to login
                let _ = remove_file(&token_path);
                bail!("Authentication expired. Please run `forklaunch login` to re-authenticate");
            }
        }
    }

    Ok(token_data.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_with(claims: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#),
            URL_SAFE_NO_PAD.encode(claims)
        )
    }

    /// The prefix is the whole of how the two credential kinds are told
    /// apart, so it is worth pinning that a JWT can never be mistaken for a
    /// key — a JWT is base64url of a JSON header, which always begins "eyJ".
    #[test]
    fn api_key_prefix_never_matches_a_jwt() {
        assert!("flk_abc123".starts_with(API_KEY_PREFIX));
        assert!(!jwt_with(r#"{"sub":"u","exp":1}"#).starts_with(API_KEY_PREFIX));
        assert!(!"eyJhbGciOiJSUzI1NiJ9.e30.sig".starts_with(API_KEY_PREFIX));
    }

    #[test]
    fn jwt_expiry_reads_exp_claim() {
        assert_eq!(
            jwt_expiry(&jwt_with(r#"{"sub":"u","exp":1789701966}"#)),
            Some(1789701966)
        );
    }

    #[test]
    fn jwt_expiry_is_none_for_tokens_without_exp_or_not_jwts() {
        assert_eq!(jwt_expiry(&jwt_with(r#"{"sub":"u"}"#)), None);
        assert_eq!(jwt_expiry("not-a-jwt"), None);
        assert_eq!(jwt_expiry("a.%%%.c"), None);
    }
}
