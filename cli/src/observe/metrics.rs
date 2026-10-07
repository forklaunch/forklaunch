use std::{collections::HashMap, io::Write};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::{get, post},
        validate::{require_integration, require_manifest},
    },
    CliCommand,
};

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct MetricsCommand;

impl MetricsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for MetricsCommand {
    fn command(&self) -> Command {
        command("metrics", "Query metrics for a ForkLaunch application")
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
                Arg::new("time_range")
                    .long("time-range")
                    .default_value("1h")
                    .value_parser(["15m", "1h", "6h", "24h", "7d", "30d"])
                    .help("Time range for metrics (15m, 1h, 6h, 24h, 7d, 30d)"),
            )
            .arg(
                Arg::new("query")
                    .long("query")
                    .help("Raw PromQL query string"),
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
        let query = matches.get_one::<String>("query").cloned();
        let json_output = matches.get_flag("json");

        // Resolve application_id: prefer --app-id flag, fall back to manifest
        let application_id = if let Some(id) = matches.get_one::<String>("app_id").cloned() {
            id
        } else {
            let (_app_root, manifest) = require_manifest(matches)?;
            require_integration(&manifest)?
        };

        if let Some(q) = query {
            let response = fetch_promql(&q, &environment, &application_id, &time_range)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_promql(&response)?;
            }
        } else {
            let response = fetch_all_application_metrics(&application_id, &environment, &time_range)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_metrics(&response)?;
            }
        }

        Ok(())
    }
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

/// The endpoint fills in exactly ONE metric per call, chosen by `chartType`
/// (see `transformToControllerMonitoring`'s `switch (chartType)`); every other
/// field is returned as a literal `0`, and `uptime` is derived as
/// `100 - errorRate`. Omitting `chartType` defaults it to `requestRate`, which
/// is why the command used to print `Latency p50: 0.0000` and
/// `Uptime: 100.0000` as if they were real measurements. Callers must request
/// each metric they intend to display — see `fetch_all_application_metrics`.
fn fetch_application_metrics(
    application_id: &str,
    environment: &str,
    time_range: &str,
    chart_type: &str,
) -> Result<ApplicationMonitoringResponse> {
    let api_url = get_observability_api_url();
    let url = format!(
        "{}/applications/{}/monitoring?environment={}&timeRange={}&chartType={}",
        api_url,
        urlencoding::encode(application_id),
        urlencoding::encode(environment),
        urlencoding::encode(time_range),
        urlencoding::encode(chart_type),
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
        .with_context(|| "Failed to parse metrics response")
}

/// Fetch each metric the display shows, since the endpoint only populates one
/// per request. A failure on any single metric is not fatal — that row simply
/// renders as "-" rather than losing the whole command.
fn fetch_all_application_metrics(
    application_id: &str,
    environment: &str,
    time_range: &str,
) -> Result<ApplicationMonitoringResponse> {
    let mut merged = ApplicationMonitoringResponse::default();
    let mut latency = LatencyMetric::default();
    let mut any_latency = false;
    let mut succeeded = false;
    let mut first_err: Option<anyhow::Error> = None;

    for chart_type in ["requestRate", "errorRate", "latencyP50", "latencyP95", "latencyP99"] {
        let r = match fetch_application_metrics(application_id, environment, time_range, chart_type)
        {
            Ok(r) => {
                succeeded = true;
                r
            }
            Err(e) => {
                // Degrade one metric to "-" rather than losing the command —
                // but never swallow a total failure (auth rejected, API down),
                // which must still surface as an error instead of a table of
                // dashes and a zero exit code.
                if first_err.is_none() {
                    first_err = Some(e);
                }
                continue;
            }
        };
        match chart_type {
            "requestRate" => merged.request_rate = r.request_rate,
            "errorRate" => {
                merged.error_rate = r.error_rate;
                // uptime is derived from errorRate server-side, so it is only
                // meaningful on the errorRate call.
                merged.uptime = r.uptime;
            }
            "latencyP50" => {
                latency.p50 = r.latency.and_then(|l| l.p50);
                any_latency |= latency.p50.is_some();
            }
            "latencyP95" => {
                latency.p95 = r.latency.and_then(|l| l.p95);
                any_latency |= latency.p95.is_some();
            }
            "latencyP99" => {
                latency.p99 = r.latency.and_then(|l| l.p99);
                any_latency |= latency.p99.is_some();
            }
            _ => {}
        }
    }

    if !succeeded {
        if let Some(e) = first_err {
            return Err(e);
        }
    }

    merged.latency = if any_latency { Some(latency) } else { None };
    Ok(merged)
}

fn fetch_promql(
    query: &str,
    environment: &str,
    application_id: &str,
    time_range: &str,
) -> Result<PromQLResponse> {
    let api_url = get_observability_api_url();
    let url = format!("{}/monitoring/promql", api_url);

    let body = serde_json::json!({
        "query": query,
        "environment": environment,
        "applicationId": application_id,
        "timeRange": time_range,
    });

    let response = post(&url, body)
        .with_context(|| "Failed to reach observability API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Observability API returned {} — {}", status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse PromQL response")
}

// ── Display ───────────────────────────────────────────────────────────────────

fn print_metrics(response: &ApplicationMonitoringResponse) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(stdout, "  Application Metrics")?;
    stdout.reset()?;
    writeln!(stdout)?;

    // Header
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stdout, "  {:<25}  {:<15}  {}", "METRIC", "VALUE", "UNIT")?;
    stdout.reset()?;

    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    writeln!(stdout, "  {:-<25}  {:-<15}  {:-<10}", "", "", "")?;
    stdout.reset()?;

    print_metric_row(&mut stdout, "Request Rate", &response.request_rate)?;
    print_metric_row(&mut stdout, "Error Rate", &response.error_rate)?;

    if let Some(latency) = &response.latency {
        if let Some(p50) = latency.p50 {
            print_metric_row(
                &mut stdout,
                "Latency p50",
                &MetricValue {
                    value: Some(p50),
                    unit: latency.unit.clone(),
                    available: latency.available,
                },
            )?;
        }
        if let Some(p95) = latency.p95 {
            print_metric_row(
                &mut stdout,
                "Latency p95",
                &MetricValue {
                    value: Some(p95),
                    unit: latency.unit.clone(),
                    available: latency.available,
                },
            )?;
        }
        if let Some(p99) = latency.p99 {
            print_metric_row(
                &mut stdout,
                "Latency p99",
                &MetricValue {
                    value: Some(p99),
                    unit: latency.unit.clone(),
                    available: latency.available,
                },
            )?;
        }
    }

    print_metric_row(&mut stdout, "Uptime", &response.uptime)?;

    writeln!(stdout)?;

    Ok(())
}

fn print_metric_row(out: &mut StandardStream, name: &str, metric: &MetricValue) -> Result<()> {
    let available = metric.available.unwrap_or(true);
    let value_str = if available {
        metric
            .value
            .map(|v| format!("{:.4}", v))
            .unwrap_or_else(|| "-".to_string())
    } else {
        "unavailable".to_string()
    };
    // No client-side unit defaults: ApplicationMonitoringSchema carries no unit
    // for these values, and guessing is worse than showing nothing. The server's
    // own numbers are req/MINUTE (monitoring-transforms.ts multiplies rate by 60)
    // and latency comes from `http_server_duration_seconds_bucket` (seconds), so
    // any hardcoded "req/s"/"ms" here would be actively wrong.
    let unit_str = metric.unit.as_deref().unwrap_or("-");

    if !available {
        out.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
    }
    writeln!(out, "  {:<25}  {:<15}  {}", name, value_str, unit_str)?;
    out.reset()?;

    Ok(())
}

fn print_promql(response: &PromQLResponse) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(stdout, "  PromQL Results  ({})", response.data.result_type)?;
    stdout.reset()?;
    writeln!(stdout)?;

    if response.data.result.is_empty() {
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        writeln!(stdout, "  No results.")?;
        stdout.reset()?;
    } else {
        for (i, series) in response.data.result.iter().enumerate() {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            write!(stdout, "  [{i}]")?;
            stdout.reset()?;

            // Print labels
            if !series.metric.is_empty() {
                let labels: Vec<String> = series
                    .metric
                    .iter()
                    .map(|(k, v)| format!("{k}={v:?}"))
                    .collect();
                writeln!(stdout, "  {{{}}}", labels.join(", "))?;
            } else {
                writeln!(stdout, "  {{<no labels>}}")?;
            }

            // Each sample is a [timestamp, value] pair; show the latest.
            match series.values.last() {
                Some(sample) if sample.len() == 2 => {
                    writeln!(stdout, "       value: {}", sample[1])?;
                    if series.values.len() > 1 {
                        writeln!(stdout, "       ({} samples)", series.values.len())?;
                    }
                }
                _ => {
                    writeln!(stdout, "       (no samples)")?;
                }
            }
        }
    }

    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricValue {
    value: Option<f64>,
    unit: Option<String>,
    available: Option<bool>,
}

/// The observability API sends these metrics as bare JSON numbers —
/// `"requestRate": 12.5` (see `ApplicationMonitoringSchema`, which types
/// `requestRate`/`errorRate`/`uptime` as `number`). Earlier CLI versions
/// assumed an object (`{value, unit, available}`), which made every
/// `observe metrics` call fail with `invalid type: integer, expected struct
/// MetricValue`. Accept both shapes so the CLI survives either side changing.
impl<'de> Deserialize<'de> for MetricValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            // An explicitly null metric renders as "-" rather than failing the
            // whole command. The schema types these as required numbers, but
            // trusting that assumption is what caused this bug in the first
            // place.
            Null,
            Scalar(f64),
            Object {
                #[serde(default)]
                value: Option<f64>,
                #[serde(default)]
                unit: Option<String>,
                #[serde(default)]
                available: Option<bool>,
            },
        }

        Ok(match Raw::deserialize(deserializer)? {
            Raw::Null => MetricValue::default(),
            Raw::Scalar(value) => MetricValue {
                value: Some(value),
                unit: None,
                available: None,
            },
            Raw::Object {
                value,
                unit,
                available,
            } => MetricValue {
                value,
                unit,
                available,
            },
        })
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatencyMetric {
    #[serde(default)]
    p50: Option<f64>,
    #[serde(default)]
    p95: Option<f64>,
    #[serde(default)]
    p99: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    available: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationMonitoringResponse {
    #[serde(default)]
    request_rate: MetricValue,
    #[serde(default)]
    error_rate: MetricValue,
    #[serde(default)]
    latency: Option<LatencyMetric>,
    #[serde(default)]
    uptime: MetricValue,
}

impl Default for MetricValue {
    fn default() -> Self {
        Self {
            value: None,
            unit: None,
            available: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromQLResultEntry {
    #[serde(default)]
    metric: HashMap<String, String>,
    /// The controller normalizes to the PLURAL key and always nests:
    /// `values: r.values ?? (r.value ? [r.value] : [])`
    /// (monitoring.controller.ts), typed `values: array(array(unknown))`.
    /// Reading the singular `value` here meant this silently deserialized to an
    /// empty Vec — no error, just every series printed with no data.
    /// Queries always go through query_range, so resultType is `matrix` and
    /// each element is a `[timestamp, value]` pair.
    #[serde(default)]
    values: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromQLData {
    result_type: String,
    result: Vec<PromQLResultEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromQLResponse {
    status: String,
    data: PromQLData,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_value_deserializes_all_fields() {
        let json = r#"{"value": 12.5, "unit": "req/s", "available": true}"#;
        let mv: MetricValue = serde_json::from_str(json).unwrap();
        assert_eq!(mv.value, Some(12.5));
        assert_eq!(mv.unit.as_deref(), Some("req/s"));
        assert_eq!(mv.available, Some(true));
    }

    #[test]
    fn metric_value_defaults_when_absent() {
        let mv: MetricValue = serde_json::from_str("{}").unwrap();
        assert!(mv.value.is_none());
        assert!(mv.unit.is_none());
        assert!(mv.available.is_none());
    }

    #[test]
    fn latency_metric_deserializes() {
        let json = r#"{"p50": 10.0, "p95": 50.0, "p99": 100.0, "unit": "ms", "available": true}"#;
        let lm: LatencyMetric = serde_json::from_str(json).unwrap();
        assert_eq!(lm.p50, Some(10.0));
        assert_eq!(lm.p95, Some(50.0));
        assert_eq!(lm.p99, Some(100.0));
        assert_eq!(lm.unit.as_deref(), Some("ms"));
    }

    #[test]
    fn promql_response_deserializes() {
        // Real shape: queries always go through query_range, so resultType is
        // `matrix` and samples arrive under the PLURAL `values` key as
        // [timestamp, value] pairs (monitoring.controller.ts normalizes this).
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "matrix",
                "result": [
                    {
                        "metric": {"__name__": "http_requests_total", "job": "api"},
                        "values": [[1700000000.0, "41"], [1700000060.0, "42"]]
                    }
                ]
            }
        }"#;
        let resp: PromQLResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "success");
        assert_eq!(resp.data.result_type, "matrix");
        assert_eq!(resp.data.result.len(), 1);
        assert_eq!(
            resp.data.result[0].metric.get("job").map(|s| s.as_str()),
            Some("api")
        );
        // latest sample is what print_promql shows
        let last = resp.data.result[0].values.last().unwrap();
        assert_eq!(last[1], serde_json::json!("42"));
        assert_eq!(resp.data.result[0].values.len(), 2);
    }

    #[test]
    fn promql_response_empty_result() {
        let json = r#"{"status": "success", "data": {"resultType": "vector", "result": []}}"#;
        let resp: PromQLResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.result.is_empty());
    }

    #[test]
    fn application_monitoring_response_deserializes() {
        let json = r#"{
            "requestRate": {"value": 10.0, "unit": "req/s", "available": true},
            "errorRate": {"value": 0.5, "unit": "%", "available": true},
            "latency": {"p50": 10.0, "p95": 50.0, "p99": 100.0, "unit": "ms", "available": true},
            "uptime": {"value": 99.9, "unit": "%", "available": true}
        }"#;
        let resp: ApplicationMonitoringResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.request_rate.value, Some(10.0));
        assert!(resp.latency.is_some());
    }

    /// Regression — the shape the API ACTUALLY returns
    /// (`ApplicationMonitoringSchema`: `requestRate`/`errorRate`/`uptime` are
    /// plain `number`, latency is `{p50,p95,p99,avg}`). This previously failed
    /// with `invalid type: integer 0, expected struct MetricValue`.
    #[test]
    fn application_monitoring_response_deserializes_scalar_metrics() {
        let json = r#"{
            "requestRate": 0,
            "requestRateTrend": "—",
            "latency": {"p50": 10.0, "p95": 50.0, "p99": 100.0, "avg": 22.0},
            "latencyTrend": "—",
            "errorRate": 0.5,
            "errorRateTrend": "+1.2%",
            "successRatePercent": 99.5,
            "available": true,
            "uptime": 99.9
        }"#;
        let resp: ApplicationMonitoringResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.request_rate.value, Some(0.0));
        assert_eq!(resp.error_rate.value, Some(0.5));
        assert_eq!(resp.uptime.value, Some(99.9));
        assert_eq!(resp.latency.as_ref().and_then(|l| l.p95), Some(50.0));
    }

    #[test]
    fn metric_value_accepts_bare_number() {
        let mv: MetricValue = serde_json::from_str("0").unwrap();
        assert_eq!(mv.value, Some(0.0));
        assert!(mv.unit.is_none());
    }

    /// A metric explicitly sent as `null` should render as "-", not abort the
    /// command. Schema says these are required numbers; this guards the case
    /// where reality disagrees (which is the whole reason this PR exists).
    #[test]
    fn metric_value_accepts_null() {
        let mv: MetricValue = serde_json::from_str("null").unwrap();
        assert!(mv.value.is_none());

        let resp: ApplicationMonitoringResponse =
            serde_json::from_str(r#"{"requestRate": null, "errorRate": 1, "uptime": 2}"#).unwrap();
        assert!(resp.request_rate.value.is_none());
        assert_eq!(resp.error_rate.value, Some(1.0));
    }

    /// A metric key absent entirely falls back to the struct default.
    #[test]
    fn application_monitoring_response_tolerates_missing_metrics() {
        let resp: ApplicationMonitoringResponse = serde_json::from_str("{}").unwrap();
        assert!(resp.request_rate.value.is_none());
        assert!(resp.latency.is_none());
    }

}
