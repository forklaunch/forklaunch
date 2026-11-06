use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use serde_json::Value;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::metrics::{ApplicationMonitoringResponse, MetricValue, fetch_all_application_metrics};

use crate::{
    CliCommand,
    core::{
        command::command,
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

        let monitoring = fetch_all_application_metrics(&application_id, &environment, "1h")?;
        let status = ObserveStatus::from_monitoring(environment, monitoring);

        if json_output {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            print_status(&status)?;
        }

        Ok(())
    }
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
            metrics: status_metrics(&monitoring),
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
        } else if self.metrics == SignalHealth::Healthy
            || self.logs == SignalHealth::Healthy
            || self.traces == SignalHealth::Healthy
        {
            SignalHealth::Healthy
        } else {
            SignalHealth::Unknown
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

fn status_metrics(monitoring: &ApplicationMonitoringResponse) -> Vec<StatusMetric> {
    let mut metrics = vec![
        status_metric("requestRate", "Request rate", &monitoring.request_rate),
        status_metric("errorRate", "Error rate", &monitoring.error_rate),
    ];
    for (name, label, value) in [
        (
            "latency.p95",
            "Latency p95",
            monitoring.latency.as_ref().and_then(|l| l.p95),
        ),
        (
            "latency.p50",
            "Latency p50",
            monitoring.latency.as_ref().and_then(|l| l.p50),
        ),
        (
            "latency.p99",
            "Latency p99",
            monitoring.latency.as_ref().and_then(|l| l.p99),
        ),
    ] {
        metrics.push(status_metric(
            name,
            label,
            &MetricValue {
                value,
                unit: monitoring.latency.as_ref().and_then(|l| l.unit.clone()),
                available: monitoring.latency.as_ref().and_then(|l| l.available),
            },
        ));
    }
    metrics.push(status_metric("uptime", "Uptime", &monitoring.uptime));
    metrics
}

fn status_metric(name: &str, label: &str, metric: &MetricValue) -> StatusMetric {
    let value = if metric.available == Some(false) {
        None
    } else {
        metric.value
    };
    let display_value = match value {
        Some(value) => match metric.unit.as_deref() {
            Some(unit) => format!("{value:.2} {unit}"),
            None => format!("{value:.2}"),
        },
        None => "unavailable".to_string(),
    };
    StatusMetric {
        name: name.to_string(),
        label: label.to_string(),
        value: value.map(Value::from).unwrap_or(Value::Null),
        display_value,
    }
}

fn classify_metrics(monitoring: &ApplicationMonitoringResponse) -> SignalHealth {
    match monitoring.available.or(monitoring.request_rate.available) {
        Some(true) => SignalHealth::Healthy,
        Some(false) | None => SignalHealth::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitoring(error_rate: f64, p95: f64, uptime: f64) -> ApplicationMonitoringResponse {
        serde_json::from_value(serde_json::json!({
            "requestRate": 12.0,
            "latency": {"p95": p95},
            "errorRate": error_rate,
            "uptime": uptime,
            "available": true
        }))
        .unwrap()
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
    fn healthy_metrics_drive_overall_status_when_other_signals_are_unknown() {
        let signals = SignalStatus {
            metrics: SignalHealth::Healthy,
            logs: SignalHealth::Unknown,
            traces: SignalHealth::Unknown,
        };

        assert_eq!(signals.overall(), SignalHealth::Healthy);
    }

    #[test]
    fn all_unknown_signals_drive_unknown_overall_status() {
        let signals = SignalStatus {
            metrics: SignalHealth::Unknown,
            logs: SignalHealth::Unknown,
            traces: SignalHealth::Unknown,
        };

        assert_eq!(signals.overall(), SignalHealth::Unknown);
    }

    #[test]
    fn omitted_availability_is_unknown() {
        let mut monitoring = monitoring(0.2, 120.0, 99.95);
        monitoring.available = None;

        assert_eq!(classify_metrics(&monitoring), SignalHealth::Unknown);
    }

    #[test]
    fn does_not_invent_units_for_scalar_api_values() {
        let status =
            ObserveStatus::from_monitoring("dev".to_string(), monitoring(0.2, 4750.0, 99.95));
        let p95 = status
            .metrics
            .iter()
            .find(|m| m.name == "latency.p95")
            .unwrap();
        assert_eq!(p95.value, Value::from(4750.0));
        assert_eq!(p95.display_value, "4750.00");
        assert!(!status.metrics[0].display_value.contains("req/s"));
    }

    #[test]
    fn missing_metric_stays_unavailable_instead_of_zero() {
        let response: ApplicationMonitoringResponse = serde_json::from_str("{}").unwrap();
        let status = ObserveStatus::from_monitoring("dev".to_string(), response);
        assert!(status.metrics.iter().all(|m| m.value.is_null()));
        assert!(
            status
                .metrics
                .iter()
                .all(|m| m.display_value == "unavailable")
        );
        assert_eq!(status.overall_status, SignalHealth::Unknown);
    }

    #[test]
    fn preserves_explicit_units_and_unavailability() {
        let response = serde_json::from_value(serde_json::json!({
            "requestRate": {"value": 12.0, "unit": "req/min", "available": true},
            "latency": {"p95": 30.0, "unit": "ms", "available": false}
        }))
        .unwrap();
        let status = ObserveStatus::from_monitoring("dev".to_string(), response);
        assert_eq!(status.metrics[0].display_value, "12.00 req/min");
        assert_eq!(status.metrics[2].display_value, "unavailable");
        assert!(status.metrics[2].value.is_null());
    }
}
