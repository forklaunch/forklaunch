use std::{collections::BTreeMap, io::Write};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug)]
pub(super) struct StatusCommand;

impl StatusCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for StatusCommand {
    fn command(&self) -> Command {
        command(
            "status",
            "Print a one-screen observability health summary for an environment",
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
        let json_output = matches.get_flag("json");

        let monitoring = fetch_application_monitoring(&application_id, &environment)?;
        let status = ObserveStatus::from_monitoring(environment, monitoring);

        if json_output {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            print_status(&status)?;
        }

        Ok(())
    }
}

fn fetch_application_monitoring(
    application_id: &str,
    environment: &str,
) -> Result<ApplicationMonitoringResponse> {
    let api_url = get_observability_api_url();
    let url = format!(
        "{}/applications/{}/monitoring?environment={}&timeRange=1h",
        api_url, application_id, environment
    );

    let auth_mode = AuthMode::detect();
    let response =
        get_with_auth(&auth_mode, &url).with_context(|| "Failed to reach observability API")?;
    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Observability API returned {} — {}", status, body);
    }

    response
        .json()
        .with_context(|| "Failed to parse observability status response")
}

fn print_status(status: &ObserveStatus) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(stdout, "Observability status for {}", status.environment)?;
    stdout.reset()?;
    writeln!(stdout)?;

    write!(stdout, "  Overall: ")?;
    write_health(&mut stdout, &status.overall_status)?;
    writeln!(stdout)?;
    writeln!(stdout)?;

    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stdout, "  Signals")?;
    stdout.reset()?;
    print_signal(&mut stdout, "Metrics", &status.signals.metrics)?;
    print_signal(&mut stdout, "Logs", &status.signals.logs)?;
    print_signal(&mut stdout, "Traces", &status.signals.traces)?;

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stdout, "  Metrics")?;
    stdout.reset()?;
    for metric in &status.metrics {
        writeln!(
            stdout,
            "    {:<18} {}",
            format!("{}:", metric.label),
            metric.display_value
        )?;
    }
    writeln!(stdout)?;

    Ok(())
}

fn print_signal(out: &mut StandardStream, label: &str, health: &SignalHealth) -> Result<()> {
    write!(out, "    {:<8} ", label)?;
    write_health(out, health)?;
    writeln!(out)?;
    Ok(())
}

fn write_health(out: &mut StandardStream, health: &SignalHealth) -> Result<()> {
    let color = match health {
        SignalHealth::Healthy => Color::Green,
        SignalHealth::Degraded => Color::Yellow,
        SignalHealth::Unhealthy => Color::Red,
        SignalHealth::Unknown => Color::White,
    };
    out.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
    write!(out, "{}", health)?;
    out.reset()?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObserveStatus {
    environment: String,
    overall_status: SignalHealth,
    signals: SignalStatus,
    metrics: Vec<StatusMetric>,
}

impl ObserveStatus {
    fn from_monitoring(environment: String, monitoring: ApplicationMonitoringResponse) -> Self {
        let metrics_health = classify_metrics(&monitoring);
        let signals = SignalStatus {
            metrics: metrics_health.clone(),
            logs: SignalHealth::Unknown,
            traces: SignalHealth::Unknown,
        };
        let overall_status = signals.overall();

        Self {
            environment,
            overall_status,
            signals,
            metrics: monitoring.metrics(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalStatus {
    metrics: SignalHealth,
    logs: SignalHealth,
    traces: SignalHealth,
}

impl SignalStatus {
    fn overall(&self) -> SignalHealth {
        if self.metrics == SignalHealth::Unhealthy
            || self.logs == SignalHealth::Unhealthy
            || self.traces == SignalHealth::Unhealthy
        {
            SignalHealth::Unhealthy
        } else if self.metrics == SignalHealth::Degraded
            || self.logs == SignalHealth::Degraded
            || self.traces == SignalHealth::Degraded
        {
            SignalHealth::Degraded
        } else if self.metrics == SignalHealth::Unknown
            || self.logs == SignalHealth::Unknown
            || self.traces == SignalHealth::Unknown
        {
            SignalHealth::Unknown
        } else {
            SignalHealth::Healthy
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum SignalHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

impl std::fmt::Display for SignalHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalHealth::Healthy => write!(f, "healthy"),
            SignalHealth::Degraded => write!(f, "degraded"),
            SignalHealth::Unhealthy => write!(f, "unhealthy"),
            SignalHealth::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusMetric {
    name: String,
    label: String,
    value: Value,
    display_value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationMonitoringResponse {
    request_rate: f64,
    latency: LatencyResponse,
    error_rate: f64,
    uptime: f64,
    #[serde(default)]
    available: Option<bool>,
    #[serde(flatten)]
    additional_metrics: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct LatencyResponse {
    p95: f64,
    #[serde(flatten)]
    additional_percentiles: BTreeMap<String, Value>,
}

impl ApplicationMonitoringResponse {
    fn metrics(&self) -> Vec<StatusMetric> {
        let mut metrics = vec![
            StatusMetric::number("requestRate", "Request rate", self.request_rate, "req/s"),
            StatusMetric::number("errorRate", "Error rate", self.error_rate, "%"),
            StatusMetric::number("latency.p95", "Latency p95", self.latency.p95, "ms"),
            StatusMetric::number("uptime", "Uptime", self.uptime, "%"),
        ];

        for (name, value) in &self.latency.additional_percentiles {
            metrics.push(StatusMetric::from_value(
                format!("latency.{name}"),
                format!("Latency {}", metric_label(name)),
                value.clone(),
                Some("ms"),
            ));
        }

        for (name, value) in &self.additional_metrics {
            metrics.push(StatusMetric::from_value(
                name.clone(),
                metric_label(name),
                value.clone(),
                None,
            ));
        }

        metrics
    }
}

impl StatusMetric {
    fn number(name: &str, label: &str, value: f64, unit: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            value: Value::from(value),
            display_value: format!("{value:.2} {unit}"),
        }
    }

    fn from_value(name: String, label: String, value: Value, unit: Option<&str>) -> Self {
        let display_value = match (&value, unit) {
            (Value::Number(number), Some(unit)) => format!("{number} {unit}"),
            (Value::Number(number), None) => number.to_string(),
            (Value::String(value), _) => value.clone(),
            (Value::Bool(value), _) => value.to_string(),
            _ => value.to_string(),
        };

        Self {
            name,
            label,
            value,
            display_value,
        }
    }
}

fn metric_label(name: &str) -> String {
    let mut label = String::new();

    for (index, char) in name.chars().enumerate() {
        if index == 0 {
            label.push(char.to_ascii_uppercase());
        } else if char == '_' || char == '-' {
            label.push(' ');
        } else if char.is_ascii_uppercase() {
            label.push(' ');
            label.push(char.to_ascii_lowercase());
        } else {
            label.push(char);
        }
    }

    label
}

fn classify_metrics(monitoring: &ApplicationMonitoringResponse) -> SignalHealth {
    if monitoring.available == Some(false) {
        return SignalHealth::Unknown;
    }

    SignalHealth::Healthy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitoring(error_rate: f64, p95: f64, uptime: f64) -> ApplicationMonitoringResponse {
        ApplicationMonitoringResponse {
            request_rate: 12.0,
            latency: LatencyResponse {
                p95,
                additional_percentiles: BTreeMap::new(),
            },
            error_rate,
            uptime,
            available: Some(true),
            additional_metrics: BTreeMap::new(),
        }
    }

    #[test]
    fn classifies_healthy_metrics() {
        assert_eq!(
            classify_metrics(&monitoring(0.2, 120.0, 99.95)),
            SignalHealth::Healthy
        );
    }

    #[test]
    fn reports_available_metrics_as_healthy_without_threshold_policy() {
        assert_eq!(
            classify_metrics(&monitoring(7.5, 1_200.0, 98.0)),
            SignalHealth::Healthy
        );
    }

    #[test]
    fn unknown_metrics_drive_overall_status() {
        let signals = SignalStatus {
            metrics: SignalHealth::Unknown,
            logs: SignalHealth::Healthy,
            traces: SignalHealth::Healthy,
        };

        assert_eq!(signals.overall(), SignalHealth::Unknown);
    }

    #[test]
    fn includes_additional_metrics_in_display_order() {
        let mut monitoring = monitoring(0.2, 120.0, 99.95);
        monitoring
            .additional_metrics
            .insert("queueDepth".to_string(), Value::from(42));

        let status = ObserveStatus::from_monitoring("dev".to_string(), monitoring);

        assert_eq!(status.metrics.len(), 5);
        assert_eq!(status.metrics[4].name, "queueDepth");
        assert_eq!(status.metrics[4].label, "Queue depth");
        assert_eq!(status.metrics[4].display_value, "42");
    }
}
