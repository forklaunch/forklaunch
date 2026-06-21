use std::{collections::HashMap, io::Write};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        hmac::AuthMode,
        http_client::get_with_auth,
        validate::{require_integration, require_manifest},
    },
};

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct TracesCommand;

impl TracesCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for TracesCommand {
    fn command(&self) -> Command {
        command("traces", "Query distributed traces for a ForkLaunch application")
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
                Arg::new("app_id")
                    .long("app-id")
                    .help("Application ID (defaults to value from forklaunch.json)"),
            )
            .arg(
                Arg::new("id")
                    .long("id")
                    .help("Trace ID — triggers detail mode for a single trace"),
            )
            .arg(
                Arg::new("limit")
                    .long("limit")
                    .default_value("50")
                    .help("Maximum number of traces to fetch"),
            )
            .arg(
                Arg::new("time_range")
                    .long("time-range")
                    .default_value("1h")
                    .value_parser(["15m", "1h", "6h", "24h", "7d", "30d"])
                    .help("Time range for traces (15m, 1h, 6h, 24h, 7d, 30d)"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?
            .to_string();
        let time_range = matches
            .get_one::<String>("time_range")
            .cloned()
            .unwrap_or_else(|| "1h".to_string());
        let limit = matches
            .get_one::<String>("limit")
            .cloned()
            .unwrap_or_else(|| "50".to_string());
        let trace_id = matches.get_one::<String>("id").cloned();
        let json_output = matches.get_flag("json");

        // Resolve application_id: prefer --app-id flag, fall back to manifest
        let application_id = if let Some(id) = matches.get_one::<String>("app_id").cloned() {
            id
        } else {
            let (_app_root, manifest) = require_manifest(matches)?;
            require_integration(&manifest)?
        };

        if let Some(tid) = trace_id {
            let response = fetch_trace_detail(&application_id, &environment, &tid)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_trace_detail(&response)?;
            }
        } else {
            let response =
                fetch_traces(&application_id, &environment, &limit, &time_range)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_traces(&response.traces)?;
            }
        }

        Ok(())
    }
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

fn fetch_traces(
    application_id: &str,
    environment: &str,
    limit: &str,
    time_range: &str,
) -> Result<TraceListResponse> {
    let api_url = get_observability_api_url();
    let url = format!(
        "{}/applications/{}/traces?environment={}&limit={}&timeRange={}",
        api_url,
        urlencoding::encode(application_id),
        urlencoding::encode(environment),
        urlencoding::encode(limit),
        urlencoding::encode(time_range),
    );

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
        .with_context(|| "Failed to parse traces response")
}

fn fetch_trace_detail(
    application_id: &str,
    environment: &str,
    trace_id: &str,
) -> Result<TraceDetailResponse> {
    let api_url = get_observability_api_url();
    let url = format!(
        "{}/applications/{}/traces/{}?environment={}",
        api_url,
        urlencoding::encode(application_id),
        urlencoding::encode(trace_id),
        urlencoding::encode(environment),
    );

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
        .with_context(|| "Failed to parse trace detail response")
}

// ── Display ───────────────────────────────────────────────────────────────────

fn print_traces(traces: &[TraceEntry]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;

    if traces.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        writeln!(stdout, "  No traces found.")?;
        stdout.reset()?;
        writeln!(stdout)?;
        return Ok(());
    }

    // Header
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<14}  {:<20}  {:<30}  {:<10}  {:<10}  {}",
        "TRACE ID", "SERVICE", "ROUTE", "DURATION", "STATUS", "START TIME"
    )?;
    stdout.reset()?;

    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(
        stdout,
        "  {:-<14}  {:-<20}  {:-<30}  {:-<10}  {:-<10}  {:-<19}",
        "", "", "", "", "", ""
    )?;
    stdout.reset()?;

    for trace in traces {
        let trace_id_short = trace.trace_id.get(..12).unwrap_or(&trace.trace_id);
        let service = if trace.service_name.chars().count() > 20 {
            format!("{}…", trace.service_name.chars().take(19).collect::<String>())
        } else {
            trace.service_name.clone()
        };
        let route = if trace.route.chars().count() > 30 {
            format!("{}…", trace.route.chars().take(29).collect::<String>())
        } else {
            trace.route.clone()
        };
        let duration = format!("{}ms", trace.duration_ms);
        let start_time = trace
            .start_time
            .get(..19)
            .map(|s| s.replace('T', " "))
            .unwrap_or_else(|| trace.start_time.clone());

        let color = status_color(&trace.status);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        write!(stdout, "  {:<14}  {:<20}  {:<30}  {:<10}  ", trace_id_short, service, route, duration)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stdout, "{:<10}", trace.status)?;
        stdout.reset()?;
        writeln!(stdout, "  {}", start_time)?;
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(stdout, "  {} trace(s) found.", traces.len())?;
    stdout.reset()?;
    writeln!(stdout)?;

    Ok(())
}

fn print_trace_detail(response: &TraceDetailResponse) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(stdout, "  Trace {}", response.trace_id)?;
    stdout.reset()?;
    writeln!(stdout)?;

    if response.spans.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        writeln!(stdout, "  No spans found.")?;
        stdout.reset()?;
        writeln!(stdout)?;
        return Ok(());
    }

    // Build parent -> children map for indentation
    let depth_map = build_depth_map(&response.spans);

    for span in &response.spans {
        let depth = depth_map.get(&span.span_id).copied().unwrap_or(0);
        let indent = "  ".repeat(depth + 1); // base 1-level indent for all spans

        let is_error = span
            .attributes
            .get("otel.status_code")
            .map(|v| v.as_str() == "ERROR")
            .unwrap_or(false);
        let color = if is_error {
            Color::Red
        } else if span.duration_ms > 1000 {
            Color::Yellow
        } else {
            Color::Green
        };

        stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stdout, "{}{}", indent, span.name)?;
        stdout.reset()?;

        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        writeln!(
            stdout,
            "  ({}, {}ms)",
            span.service_name, span.duration_ms
        )?;
        stdout.reset()?;
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(stdout, "  {} span(s).", response.spans.len())?;
    stdout.reset()?;
    writeln!(stdout)?;

    Ok(())
}

/// Returns a map from spanId -> depth (root = 0).
fn build_depth_map(spans: &[Span]) -> HashMap<String, usize> {
    // Build parent -> children adjacency
    let mut children: HashMap<Option<String>, Vec<&Span>> = HashMap::new();
    for span in spans {
        children
            .entry(span.parent_span_id.clone())
            .or_default()
            .push(span);
    }

    let mut depth_map: HashMap<String, usize> = HashMap::new();
    // BFS from roots (parent_span_id == None)
    let mut queue: Vec<(&Span, usize)> = children
        .get(&None)
        .map(|roots| roots.iter().map(|s| (*s, 0usize)).collect())
        .unwrap_or_default();

    while let Some((span, depth)) = queue.pop() {
        depth_map.insert(span.span_id.clone(), depth);
        if let Some(kids) = children.get(&Some(span.span_id.clone())) {
            for kid in kids {
                queue.push((kid, depth + 1));
            }
        }
    }

    depth_map
}

fn status_color(status: &str) -> Color {
    match status.to_lowercase().as_str() {
        "ok" | "success" | "200" => Color::Green,
        "error" | "500" | "503" => Color::Red,
        s if s.starts_with('4') => Color::Yellow,
        s if s.starts_with('5') => Color::Red,
        _ => Color::White,
    }
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceEntry {
    trace_id: String,
    service_name: String,
    route: String,
    duration_ms: u64,
    status: String,
    start_time: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceListResponse {
    traces: Vec<TraceEntry>,
    #[serde(default)]
    available: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Span {
    span_id: String,
    #[serde(default)]
    parent_span_id: Option<String>,
    name: String,
    service_name: String,
    start_time: String,
    duration_ms: u64,
    #[serde(default)]
    attributes: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceDetailResponse {
    trace_id: String,
    spans: Vec<Span>,
    #[serde(default)]
    available: bool,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_span(span_id: &str, parent_span_id: Option<&str>, name: &str) -> Span {
        Span {
            span_id: span_id.to_string(),
            parent_span_id: parent_span_id.map(|s| s.to_string()),
            name: name.to_string(),
            service_name: "svc".to_string(),
            start_time: "2024-01-15T10:00:00Z".to_string(),
            duration_ms: 100,
            attributes: HashMap::new(),
        }
    }

    #[test]
    fn status_color_ok_is_green() {
        assert_eq!(status_color("ok"), Color::Green);
        assert_eq!(status_color("success"), Color::Green);
    }

    #[test]
    fn status_color_error_is_red() {
        assert_eq!(status_color("error"), Color::Red);
        assert_eq!(status_color("500"), Color::Red);
    }

    #[test]
    fn status_color_4xx_is_yellow() {
        assert_eq!(status_color("404"), Color::Yellow);
    }

    #[test]
    fn build_depth_map_root_is_zero() {
        let spans = vec![make_span("root", None, "root-op")];
        let map = build_depth_map(&spans);
        assert_eq!(map["root"], 0);
    }

    #[test]
    fn build_depth_map_child_is_one() {
        let spans = vec![
            make_span("root", None, "root-op"),
            make_span("child", Some("root"), "child-op"),
        ];
        let map = build_depth_map(&spans);
        assert_eq!(map["root"], 0);
        assert_eq!(map["child"], 1);
    }

    #[test]
    fn build_depth_map_grandchild_is_two() {
        let spans = vec![
            make_span("root", None, "root-op"),
            make_span("child", Some("root"), "child-op"),
            make_span("grand", Some("child"), "grand-op"),
        ];
        let map = build_depth_map(&spans);
        assert_eq!(map["grand"], 2);
    }

    #[test]
    fn trace_entry_deserializes() {
        let json = r#"{
            "traceId": "abc123",
            "serviceName": "api",
            "route": "/health",
            "durationMs": 42,
            "status": "ok",
            "startTime": "2024-01-15T10:00:00Z"
        }"#;
        let entry: TraceEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.trace_id, "abc123");
        assert_eq!(entry.duration_ms, 42);
    }

    #[test]
    fn span_deserializes_with_optional_parent() {
        let json = r#"{
            "spanId": "s1",
            "name": "http.request",
            "serviceName": "api",
            "startTime": "2024-01-15T10:00:00Z",
            "durationMs": 55
        }"#;
        let span: Span = serde_json::from_str(json).unwrap();
        assert_eq!(span.span_id, "s1");
        assert!(span.parent_span_id.is_none());
    }

    #[test]
    fn trace_list_response_deserializes() {
        let json = r#"{
            "traces": [
                {"traceId":"t1","serviceName":"api","route":"/","durationMs":10,"status":"ok","startTime":"2024-01-15T10:00:00Z"}
            ],
            "available": true
        }"#;
        let resp: TraceListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.traces.len(), 1);
        assert!(resp.available);
    }

    #[test]
    fn trace_detail_response_deserializes() {
        let json = r#"{
            "traceId": "trace-001",
            "spans": [
                {"spanId":"s1","name":"root","serviceName":"api","startTime":"2024-01-15T10:00:00Z","durationMs":200}
            ],
            "available": true
        }"#;
        let resp: TraceDetailResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.trace_id, "trace-001");
        assert_eq!(resp.spans.len(), 1);
    }
}
