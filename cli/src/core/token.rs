use std::{
    env::var,
    fs::{read_to_string, remove_file, write},
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::constants::get_iam_api_url;

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

    Ok(TokenData {
        access_token: jwt_data.token,
        refresh_token: jwt_data.refresh_token.unwrap_or_default(),
        expires_at,
    })
}

fn save_token_data(token_data: &TokenData) -> Result<()> {
    let token_path = get_token_path()?;
    let toml_content = toml::to_string(token_data)?;
    write(&token_path, toml_content)?;
    Ok(())
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
