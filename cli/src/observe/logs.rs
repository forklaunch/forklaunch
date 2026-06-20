use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use tungstenite::client::IntoClientRequest;

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        hmac::AuthMode,
        http_client::get_with_auth,
        token::get_token,
        validate::{require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(super) struct LogsCommand;

impl LogsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for LogsCommand {
    fn command(&self) -> Command {
        command("logs", "Query or live-tail logs for a ForkLaunch application")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("environment")
                    .short('e')
                    .long("environment")
                    .required(true)
                    .help("Environment to inspect (for example: dev, staging, production)"),
            )
            .arg(
                Arg::new("service")
                    .short('s')
                    .long("service")
                    .help("Filter to a specific service name"),
            )
            .arg(
                Arg::new("level")
                    .long("level")
                    .help("Filter by log level (error, warn, info, debug)"),
            )
            .arg(
                Arg::new("since")
                    .long("since")
                    .help("Return logs newer than this ISO timestamp"),
            )
            .arg(
                Arg::new("limit")
                    .long("limit")
                    .default_value("100")
                    .value_parser(clap::value_parser!(u32))
                    .help("Maximum number of log lines to fetch"),
            )
            .arg(
                Arg::new("follow")
                    .short('f')
                    .long("follow")
                    .help("Stream new logs as they arrive (live-tail)")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?
            .to_string();
        let service = matches.get_one::<String>("service").cloned();
        let level = matches.get_one::<String>("level").cloned();
        let since = matches.get_one::<String>("since").cloned();
        let limit: u32 = matches.get_one::<u32>("limit").copied().unwrap_or(100);
        let follow = matches.get_flag("follow");
        let json_output = matches.get_flag("json");

        if follow {
            stream_logs(
                &application_id,
                &environment,
                service.as_deref(),
                level.as_deref(),
                limit,
                json_output,
            )
        } else {
            query_logs(
                &application_id,
                &environment,
                service.as_deref(),
                level.as_deref(),
                since.as_deref(),
                limit,
                json_output,
            )
        }
    }
}

// ── HTTP query (no --follow) ──────────────────────────────────────────────────

fn query_logs(
    application_id: &str,
    environment: &str,
    service: Option<&str>,
    level: Option<&str>,
    since: Option<&str>,
    limit: u32,
    json_output: bool,
) -> Result<()> {
    let response = fetch_logs(application_id, environment, service, level, since, limit)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_logs(&response.logs)?;
    }

    Ok(())
}

fn fetch_logs(
    application_id: &str,
    environment: &str,
    service: Option<&str>,
    level: Option<&str>,
    since: Option<&str>,
    limit: u32,
) -> Result<LogsResponse> {
    let api_url = get_observability_api_url();
    let mut url = format!(
        "{}/applications/{}/logs?environment={}&limit={}&direction=backward",
        api_url,
        application_id,
        urlencoding::encode(environment),
        limit
    );
    if let Some(svc) = service {
        url.push_str(&format!("&service={}", urlencoding::encode(svc)));
    }
    if let Some(lvl) = level {
        url.push_str(&format!("&level={}", urlencoding::encode(lvl)));
    }
    if let Some(s) = since {
        url.push_str(&format!("&since={}", urlencoding::encode(s)));
    }

    let auth_mode = AuthMode::detect();
    let response =
        get_with_auth(&auth_mode, &url).with_context(|| "Failed to reach observability API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Observability API returned {} — {}", status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse logs response")
}

// ── WebSocket follow (--follow) ───────────────────────────────────────────────

fn stream_logs(
    application_id: &str,
    environment: &str,
    service: Option<&str>,
    level: Option<&str>,
    limit: u32,
    json_output: bool,
) -> Result<()> {
    // Fetch one page first so we print recent history and know the starting timestamp
    let initial = fetch_logs(application_id, environment, service, level, None, limit)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&initial)?);
    } else {
        print_logs(&initial.logs)?;
    }

    let ws_url = build_ws_url()?;
    let mut request = ws_url
        .as_str()
        .into_client_request()
        .with_context(|| "Failed to build WebSocket request")?;

    // Attach auth header
    let auth_mode = AuthMode::detect();
    match &auth_mode {
        AuthMode::Jwt => {
            let token = get_token().with_context(|| "Failed to get auth token")?;
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", token)
                    .parse()
                    .with_context(|| "Invalid auth header value")?,
            );
        }
        AuthMode::Hmac { secret_key } => {
            let auth_header = crate::core::hmac::generate_hmac_auth_header(
                secret_key,
                "GET",
                "/ws",
                None,
            )?;
            request.headers_mut().insert(
                "Authorization",
                auth_header
                    .parse()
                    .with_context(|| "Invalid HMAC header value")?,
            );
        }
    }

    let (mut socket, _) =
        tungstenite::connect(request).with_context(|| "Failed to connect to WebSocket")?;

    // Subscribe to logs channel
    let subscribe_msg = serde_json::json!({
        "type": "subscribeLogs",
        "applicationId": application_id,
        "environment": environment,
        "serviceName": service,
        "level": level,
    });
    socket
        .send(tungstenite::Message::Text(
            subscribe_msg.to_string().into(),
        ))
        .with_context(|| "Failed to send subscribe message")?;

    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    if !json_output {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Streaming logs for {} ({})…  Ctrl+C to stop", application_id, environment)?;
        stdout.reset()?;
        writeln!(stdout)?;
    }

    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                let msg: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Only handle channelData messages that carry logs
                if msg.get("channel").is_none() {
                    continue;
                }
                let data = match msg.get("data") {
                    Some(d) => d,
                    None => continue,
                };
                let logs: Vec<LogEntry> = match data.get("logs") {
                    Some(l) => match serde_json::from_value(l.clone()) {
                        Ok(entries) => entries,
                        Err(_) => continue,
                    },
                    None => continue,
                };

                if json_output {
                    println!("{}", serde_json::to_string_pretty(&logs)?);
                } else {
                    print_logs(&logs)?;
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::ConnectionClosed) => break,
            Err(e) => {
                anyhow::bail!("WebSocket error: {}", e);
            }
        }
    }

    Ok(())
}

fn build_ws_url() -> Result<String> {
    let api_url = get_observability_api_url();
    // Convert http(s):// to ws(s):// and append /ws path
    let ws_url = if api_url.starts_with("https://") {
        format!("wss://{}/ws", &api_url[8..])
    } else if api_url.starts_with("http://") {
        format!("ws://{}/ws", &api_url[7..])
    } else {
        format!("ws://{}/ws", api_url)
    };
    Ok(ws_url)
}

// ── Display ───────────────────────────────────────────────────────────────────

fn print_logs(logs: &[LogEntry]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    for entry in logs {
        let level = entry.level.as_deref().unwrap_or("info");
        let color = level_color(level);

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        let ts_display = entry.timestamp.get(..19).unwrap_or(&entry.timestamp).replace('T', " ");
        write!(stdout, "{} ", ts_display)?;

        stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stdout, "{:<5} ", level.to_uppercase())?;

        stdout.reset()?;
        writeln!(stdout, "{}", entry.message)?;
    }

    Ok(())
}

fn level_color(level: &str) -> Color {
    match level.to_lowercase().as_str() {
        "error" => Color::Red,
        "warn" | "warning" => Color::Yellow,
        "info" => Color::Green,
        "debug" => Color::Cyan,
        _ => Color::White,
    }
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    id: String,
    timestamp: String,
    level: Option<String>,
    message: String,
    #[serde(default)]
    labels: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsResponse {
    logs: Vec<LogEntry>,
    #[serde(default)]
    available: bool,
    #[serde(default)]
    has_more: Option<bool>,
    #[serde(default)]
    next_cursor: Option<String>,
}
