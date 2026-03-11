use std::{
    fs::OpenOptions,
    io::Write,
    thread::sleep,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

#[cfg(not(unix))]
use std::fs::create_dir_all;

use anyhow::{Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_iam_api_url,
    core::{command::command, token::get_token_path},
};

pub(super) struct LoginCommand;

impl LoginCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    interval: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenData {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

/// Login with API token (for automation/CI)
/// This accepts a long-lived API token that users generate from the platform UI
pub fn login_with_token(api_token: &str) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    log_info!(stdout, "Forklaunch CLI Login (API Token)");
    log_info!(stdout, "Validating API token...");

    // The API token is already a JWT that can be used directly
    // We just need to validate it and save it
    let token_storage = TokenData {
        access_token: api_token.to_string(),
        refresh_token: String::new(), // API tokens don't have refresh tokens
        expires_at: i64::MAX, // API tokens are long-lived
    };

    let token_path = get_token_path()?;

    // Ensure parent directory exists with owner-only permissions (0o700)
    if let Some(parent) = token_path.parent() {
        #[cfg(unix)]
        {
            use std::fs::DirBuilder;
            let mut builder = DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            builder.create(parent)?;
        }

        #[cfg(not(unix))]
        {
            create_dir_all(parent)?;
        }
    }

    let toml_content = toml::to_string(&token_storage)?;

    // Write token file with owner-only permissions (0o600)
    #[cfg(unix)]
    {
        use std::io::Write as IoWrite;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&token_path)?;

        file.write_all(toml_content.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        use std::fs::write;
        write(&token_path, toml_content)?;
    }

    writeln!(stdout)?;
    log_header!(stdout, Color::Green, "Successfully logged in with API token!");
    writeln!(
        stdout,
        "Note: API tokens are long-lived. Revoke them from the platform UI if compromised."
    )?;

    Ok(())
}

/// Interactive device flow login (default)
pub fn login() -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    let api_url = get_iam_api_url();

    // Step 1: Request device code
    log_info!(stdout, "Forklaunch CLI Login");
    log_info!(stdout, "Requesting device authorization...");

    let client = reqwest::blocking::Client::new();
    let device_response = client
        .post(format!("{}/api/auth/device/code", api_url))
        .json(&serde_json::json!({
            "client_id": "forklaunch-cli",
            "scope": "openid profile email"
        }))
        .send()?;

    if !device_response.status().is_success() {
        bail!(
            "Failed to request device code: {}",
            device_response.status()
        );
    }

    let device_data: DeviceCodeResponse = device_response.json()?;

    // Step 2: Display user code and open browser
    writeln!(stdout)?;
    log_header!(stdout, Color::Yellow, "Please visit: {}", device_data.verification_uri);
    log_header!(stdout, Color::Yellow, "Enter code: {}", device_data.user_code);
    writeln!(stdout)?;

    // Try to open browser
    let url_to_open = device_data
        .verification_uri_complete
        .as_ref()
        .unwrap_or(&device_data.verification_uri);

    log_info!(stdout, "Opening browser...");

    if let Err(e) = opener::open(url_to_open) {
        log_warn!(stdout, "Could not open browser automatically: {}", e);
        log_warn!(stdout, "Please open the URL manually.");
    }

    // Step 3: Poll for token
    let interval = Duration::from_secs(device_data.interval.unwrap_or(5) as u64);
    let mut polling_interval = interval;

    log_info!(stdout, "Waiting for authorization...");

    loop {
        sleep(polling_interval);

        let token_response = client
            .post(format!("{}/api/auth/device/token", api_url))
            .json(&serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": device_data.device_code,
                "client_id": "forklaunch-cli"
            }))
            .send()?;

        if token_response.status().is_success() {
            // Got session token from device auth - now exchange for JWT
            let response_body = token_response.text()?;

            let token_data: TokenResponse = serde_json::from_str(&response_body)?;
            let session_token = token_data.access_token;

            // Call /api/auth/token with session cookie to get JWT (same as browser)
            log_info!(stdout, "Exchanging session for JWT...");

            let jwt_url = format!("{}/api/auth/token", api_url);

            let jwt_response = client
                .get(&jwt_url)
                // Use Bearer auth with the session token (supported by bearer plugin)
                .bearer_auth(&session_token)
                .send()?;

            if !jwt_response.status().is_success() {
                let status = jwt_response.status();
                let body = jwt_response.text().unwrap_or_default();
                bail!("Failed to get JWT: {} - {}", status, body);
            }

            let jwt_body = jwt_response.text()?;

            #[derive(Deserialize)]
            struct JwtResponse {
                token: String,
                #[serde(rename = "expiresIn")]
                expires_in: Option<i64>,
            }

            let jwt_data: JwtResponse = serde_json::from_str(&jwt_body)?;
            let expires_at = chrono::Utc::now().timestamp() + jwt_data.expires_in.unwrap_or(604800);

            let token_storage = TokenData {
                access_token: jwt_data.token,
                refresh_token: session_token, // Use session token as refresh token
                expires_at,
            };

            let token_path = get_token_path()?;

            // Ensure parent directory exists with owner-only permissions (0o700)
            if let Some(parent) = token_path.parent() {
                #[cfg(unix)]
                {
                    use std::fs::DirBuilder;
                    let mut builder = DirBuilder::new();
                    builder.recursive(true);
                    builder.mode(0o700);
                    builder.create(parent)?;
                }

                #[cfg(not(unix))]
                {
                    create_dir_all(parent)?;
                }
            }

            let toml_content = toml::to_string(&token_storage)?;

            // Write token file with owner-only permissions (0o600)
            #[cfg(unix)]
            {
                use std::io::Write as IoWrite;

                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&token_path)?;

                file.write_all(toml_content.as_bytes())?;
            }

            #[cfg(not(unix))]
            {
                use std::fs::write;
                write(&token_path, toml_content)?;
            }

            writeln!(stdout)?;
            log_header!(stdout, Color::Green, "Successfully logged in!");

            return Ok(());
        } else {
            let error_data: Result<TokenErrorResponse, _> = token_response.json();

            match error_data {
                Ok(error) => {
                    match error.error.as_str() {
                        "authorization_pending" => {
                            continue;
                        }
                        "slow_down" => {
                            polling_interval += Duration::from_secs(5);
                            log_warn!(stdout, "Slowing down polling to {}s", polling_interval.as_secs());
                            continue;
                        }
                        "access_denied" => {
                            bail!("Access was denied by the user");
                        }
                        "expired_token" => {
                            bail!("The device code has expired. Please try again.");
                        }
                        _ => {
                            bail!("Error: {}", error.error_description.unwrap_or(error.error));
                        }
                    }
                }
                Err(_) => {
                    bail!("Failed to authenticate: unexpected response");
                }
            }
        }
    }
}

impl CliCommand for LoginCommand {
    fn command(&self) -> Command {
        command("login", "Login to the forklaunch platform")
            .arg(
                Arg::new("token")
                    .long("token")
                    .short('t')
                    .value_name("API_TOKEN")
                    .help("API token for headless authentication (for CI/CD). Can also be set via FORKLAUNCH_API_TOKEN environment variable"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        if let Some(token) = matches.get_one::<String>("token") {
            return login_with_token(token);
        }

        if let Ok(token) = std::env::var("FORKLAUNCH_API_TOKEN") {
            return login_with_token(&token);
        }

        login()
    }
}
