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
        http_client::{get_with_auth, post_with_auth},
        validate::{require_integration, require_manifest},
    },
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
            let response =
                fetch_promql(&q, &environment, &application_id, &time_range)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_promql(&response)?;
            }
        } else {
            let response =
                fetch_application_metrics(&application_id, &environment, &time_range)?;
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

fn fetch_application_metrics(
    application_id: &str,
    environment: &str,
    time_range: &str,
) -> Result<ApplicationMonitoringResponse> {
    let api_url = get_observability_api_url();
    let url = format!(
        "{}/applications/{}/monitoring?environment={}&timeRange={}",
        api_url,
        urlencoding::encode(application_id),
        urlencoding::encode(environment),
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
        .with_context(|| "Failed to parse metrics response")
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

    let auth_mode = AuthMode::detect();
    let response =
        post_with_auth(&auth_mode, &url, body).with_context(|| "Failed to reach observability API")?;

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

            // Value tuple: [timestamp_f64, value_string]
            if series.value.len() == 2 {
                writeln!(stdout, "       value: {}", series.value[1])?;
            }
        }
    }

    writeln!(stdout)?;

    Ok(())
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricValue {
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    available: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
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
    #[serde(default)]
    value: Vec<serde_json::Value>,
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
        let json = r#"{
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {"__name__": "http_requests_total", "job": "api"},
                        "value": [1700000000.0, "42"]
                    }
                ]
            }
        }"#;
        let resp: PromQLResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "success");
        assert_eq!(resp.data.result_type, "vector");
        assert_eq!(resp.data.result.len(), 1);
        assert_eq!(
            resp.data.result[0].metric.get("job").map(|s| s.as_str()),
            Some("api")
        );
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
}
