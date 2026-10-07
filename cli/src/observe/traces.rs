use std::{
    collections::{HashMap, HashSet},
    io::Write,
};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::get,
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
        command(
            "traces",
            "Query distributed traces for a ForkLaunch application",
        )
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
                .help("Application ID (defaults to value from .forklaunch/manifest.toml)"),
        )
        .arg(
            Arg::new("trace_id")
                .long("trace-id")
                .help("Trace ID — show full span tree for a single trace"),
        )
        .arg(
            Arg::new("limit")
                .long("limit")
                .default_value("50")
                .value_parser(clap::value_parser!(u32).range(1..))
                .help("Maximum number of traces to fetch (list mode only)"),
        )
        .arg(
            Arg::new("time_range")
                .long("time-range")
                .default_value("1h")
                .value_parser(["15m", "1h", "6h", "24h", "7d", "30d"])
                .help("Time range for traces (list mode only): 15m, 1h, 6h, 24h, 7d, 30d"),
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
        let json_output = matches.get_flag("json");

        let application_id = if let Some(id) = matches.get_one::<String>("app_id").cloned() {
            id
        } else {
            let (_app_root, manifest) = require_manifest(matches)?;
            require_integration(&manifest)?
        };

        if let Some(trace_id) = matches.get_one::<String>("trace_id").cloned() {
            let response = fetch_trace_detail(&application_id, &environment, &trace_id)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_trace_detail(&response)?;
            }
        } else {
            let limit = matches
                .get_one::<u32>("limit")
                .copied()
                .unwrap_or(50)
                .to_string();
            let time_range = matches
                .get_one::<String>("time_range")
                .cloned()
                .unwrap_or_else(|| "1h".to_string());
            let response = fetch_traces(&application_id, &environment, &limit, &time_range)?;
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

    let response = get(&url).with_context(|| "Failed to reach observability API")?;

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

    let response = get(&url).with_context(|| "Failed to reach observability API")?;

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

/// Shown where the API omitted an optional field (route, service, status).
const MISSING: &str = "—";

/// Truncate to `max` display characters, appending an ellipsis when clipped.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

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
        // service/route/status are optional server-side — show a placeholder
        // instead of failing or leaving the column blank.
        let service = truncate(trace.service_name.as_deref().unwrap_or(MISSING), 20);
        let route = truncate(trace.route.as_deref().unwrap_or(MISSING), 30);
        let status = trace.status.as_deref().unwrap_or(MISSING);
        let duration = format!("{:.0}ms", trace.duration_ms);
        let start_time = trace
            .start_time
            .get(..19)
            .map(|s| s.replace('T', " "))
            .unwrap_or_else(|| trace.start_time.clone());

        let color = status_color(status);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        write!(
            stdout,
            "  {:<14}  {:<20}  {:<30}  {:<10}  ",
            trace_id_short, service, route, duration
        )?;
        stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(stdout, "{:<10}", status)?;
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

    // Render in tree order (parent before its children), not the raw order the
    // API returned them in — otherwise a child can print above its parent.
    for (span, depth) in build_ordered_spans(&response.spans) {
        let indent = "  ".repeat(depth + 1);

        let is_error = span
            .attributes
            .get("otel.status_code")
            .and_then(|v| v.as_str())
            .map(|s| s == "ERROR")
            .unwrap_or(false);
        let color = if is_error {
            Color::Red
        } else if span.duration_ms > 1000.0 {
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
            "  ({}, {:.2}ms)",
            span.service_name.as_deref().unwrap_or(MISSING),
            span.duration_ms
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

/// Returns spans in DFS pre-order (parent immediately before its children),
/// each paired with its depth (root = 0). Siblings keep their original order.
/// Orphaned spans — whose parent isn't in the set — are appended at the end at
/// depth 0 so a partial trace still renders every span instead of dropping some.
fn build_ordered_spans(spans: &[Span]) -> Vec<(&Span, usize)> {
    let mut children: HashMap<Option<String>, Vec<&Span>> = HashMap::new();
    for span in spans {
        children
            .entry(span.parent_span_id.clone())
            .or_default()
            .push(span);
    }

    let mut ordered: Vec<(&Span, usize)> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Stack-based DFS. Push siblings in reverse so they pop in original order
    // and each subtree is emitted fully before the next sibling.
    let mut stack: Vec<(&Span, usize)> = Vec::new();
    if let Some(roots) = children.get(&None) {
        for root in roots.iter().rev() {
            stack.push((root, 0));
        }
    }
    while let Some((span, depth)) = stack.pop() {
        if !visited.insert(span.span_id.clone()) {
            continue;
        }
        ordered.push((span, depth));
        if let Some(kids) = children.get(&Some(span.span_id.clone())) {
            for kid in kids.iter().rev() {
                stack.push((kid, depth + 1));
            }
        }
    }

    // Append orphans (parent not present in the trace) so nothing is dropped.
    for span in spans {
        if !visited.contains(&span.span_id) {
            ordered.push((span, 0));
        }
    }

    ordered
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

/// Mirrors the API's trace summary. `serviceName`, `route` and `status` are
/// declared `.optional()` server-side (see `ServiceTracesResponseSchema`) —
/// not every span is an HTTP request, so a trace can legitimately arrive
/// without a route. Treating them as required made the whole command fail with
/// `missing field \`route\`` the moment one such trace appeared.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceEntry {
    trace_id: String,
    #[serde(default)]
    service_name: Option<String>,
    #[serde(default)]
    route: Option<String>,
    /// Fractional: the server computes this as (endNano-startNano)/1e6 with no
    /// rounding, so it is not an integer in general. `u64` rejected every float
    /// value serde saw — including whole ones like `12.0`.
    duration_ms: f64,
    #[serde(default)]
    status: Option<String>,
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
    /// `.optional()` server-side, same as on `TraceEntry` — required here would
    /// break `observe traces <id>` on any span without a service name.
    #[serde(default)]
    service_name: Option<String>,
    start_time: String,
    /// Fractional: the server computes this as (endNano-startNano)/1e6 with no
    /// rounding, so it is not an integer in general. `u64` rejected every float
    /// value serde saw — including whole ones like `12.0`.
    duration_ms: f64,
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

    /// spanId -> depth, derived from the tree walk. Test-only convenience.
    fn build_depth_map(spans: &[Span]) -> HashMap<String, usize> {
        build_ordered_spans(spans)
            .into_iter()
            .map(|(span, depth)| (span.span_id.clone(), depth))
            .collect()
    }

    fn make_span(span_id: &str, parent_span_id: Option<&str>, name: &str) -> Span {
        Span {
            span_id: span_id.to_string(),
            parent_span_id: parent_span_id.map(|s| s.to_string()),
            name: name.to_string(),
            service_name: Some("svc".to_string()),
            start_time: "2024-01-15T10:00:00Z".to_string(),
            duration_ms: 100.0,
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
    fn ordered_spans_are_tree_order_not_input_order() {
        // API returns children before parents (chronological-ish). The tree
        // walk must still emit each parent before its own children.
        let spans = vec![
            make_span("grand", Some("child"), "grand-op"),
            make_span("child", Some("root"), "child-op"),
            make_span("root", None, "root-op"),
        ];
        let order: Vec<&str> = build_ordered_spans(&spans)
            .iter()
            .map(|(s, _)| s.span_id.as_str())
            .collect();
        assert_eq!(order, vec!["root", "child", "grand"]);
    }

    #[test]
    fn ordered_spans_group_subtrees_before_siblings() {
        // root has two children; each child's subtree must be emitted fully
        // before moving to the next sibling.
        let spans = vec![
            make_span("root", None, "root-op"),
            make_span("a", Some("root"), "a-op"),
            make_span("b", Some("root"), "b-op"),
            make_span("a1", Some("a"), "a1-op"),
        ];
        let order: Vec<&str> = build_ordered_spans(&spans)
            .iter()
            .map(|(s, _)| s.span_id.as_str())
            .collect();
        assert_eq!(order, vec!["root", "a", "a1", "b"]);
    }

    #[test]
    fn ordered_spans_keep_orphans() {
        // A span whose parent isn't present must still render, not vanish.
        let spans = vec![
            make_span("root", None, "root-op"),
            make_span("orphan", Some("missing"), "orphan-op"),
        ];
        let ids: Vec<&str> = build_ordered_spans(&spans)
            .iter()
            .map(|(s, _)| s.span_id.as_str())
            .collect();
        assert!(ids.contains(&"root"));
        assert!(ids.contains(&"orphan"));
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
        assert_eq!(entry.duration_ms, 42.0);
        assert_eq!(entry.route.as_deref(), Some("/health"));
    }

    /// Regression: the server computes durations as
    /// `(endNano - startNano) / 1_000_000` with no rounding
    /// (traces.service.ts durationFromNano), so `durationMs` is fractional in
    /// general. `u64` rejected every float — including whole ones like `12.0`
    /// — which broke `observe traces <trace-id>` entirely.
    #[test]
    fn fractional_durations_deserialize() {
        let span: Span = serde_json::from_str(
            r#"{"spanId":"s1","name":"http.request","startTime":"2024-01-15T10:00:00Z","durationMs":55.45556640625}"#,
        )
        .unwrap();
        assert!((span.duration_ms - 55.45556640625).abs() < f64::EPSILON);

        let entry: TraceEntry = serde_json::from_str(
            r#"{"traceId":"t1","startTime":"2024-01-15T10:00:00Z","durationMs":12.0}"#,
        )
        .unwrap();
        assert!((entry.duration_ms - 12.0).abs() < f64::EPSILON);
    }

    /// Regression: `route`, `serviceName` and `status` are `.optional()`
    /// server-side. A non-HTTP trace arrives without them and previously blew
    /// the whole command up with `missing field \`route\``.
    #[test]
    fn trace_entry_deserializes_without_optional_fields() {
        let json = r#"{
            "traceId": "abc123",
            "durationMs": 42,
            "startTime": "2024-01-15T10:00:00Z"
        }"#;
        let entry: TraceEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.trace_id, "abc123");
        assert!(entry.route.is_none());
        assert!(entry.service_name.is_none());
        assert!(entry.status.is_none());
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
