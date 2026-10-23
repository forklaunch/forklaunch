//! Opt-in, application-owned OTLP/HTTP providers. Call [`TelemetryRuntime::start`](crate::telemetry::otlp::TelemetryRuntime::start)
//! once from an executable, after handling offline contract export. Libraries
//! should accept a collector instead of installing process-global providers.
//!
//! `RUST_LOG` is required and sets the stderr log filter.
//! Set `OTEL_EXPORTER_OTLP_ENDPOINT` (or a signal-specific endpoint) to enable
//! export. Standard OTLP headers, resource attributes and signal-specific endpoint
//! settings are read by the upstream exporters. HTTP/protobuf (default) and
//! HTTP/JSON are supported. `OTEL_*_EXPORTER=none` disables individual signals;
//! `OTEL_SDK_DISABLED=true` disables all export. Logs still go to JSON stderr.
//! Providers run on SDK background threads. Startup, flush and shutdown run on
//! Tokio's blocking pool so blocking HTTP clients never block an async executor.
use crate::common::{FrameworkError, FrameworkResult};
use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::{
    Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use std::{
    env,
    sync::atomic::{AtomicBool, Ordering},
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Owns the executable's providers. Explicit shutdown flushes every enabled
/// signal, including on service/worker failure. Global handles are installed only
/// by this explicit application API; constructing framework collectors never
/// replaces providers. A second installation is rejected.
pub struct TelemetryRuntime {
    traces: Option<SdkTracerProvider>,
    metrics: Option<SdkMeterProvider>,
    logs: Option<SdkLoggerProvider>,
}

impl TelemetryRuntime {
    /// Initializes structured stderr logging and any configured OTLP providers.
    ///
    /// # Errors
    /// Rejects invalid configuration, unsupported exporters/protocols, or an
    /// already installed tracing subscriber/runtime. Errors omit endpoints and
    /// headers, which may contain credentials.
    pub async fn start(
        service: impl Into<String>,
        version: impl Into<String>,
    ) -> FrameworkResult<Self> {
        let service = service.into();
        let version = version.into();
        tokio::task::spawn_blocking(move || Self::install(&service, &version))
            .await
            .map_err(|_| error("telemetry_start", "Telemetry startup failed"))?
    }

    fn install(service: &str, version: &str) -> FrameworkResult<Self> {
        let config = Configuration::from_env()?;
        let filter: String = crate::config::required("RUST_LOG")?;
        let filter = EnvFilter::try_new(filter)
            .map_err(|_| error("telemetry_config", "Invalid logging filter"))?;
        if INSTALLED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(error(
                "telemetry_installed",
                "Telemetry is already installed",
            ));
        }
        match Self::build_and_install(service, version, config, filter) {
            Ok(runtime) => Ok(runtime),
            Err(error) => {
                INSTALLED.store(false, Ordering::Release);
                Err(error)
            }
        }
    }

    fn build_and_install(
        service: &str,
        version: &str,
        config: Configuration,
        filter: EnvFilter,
    ) -> FrameworkResult<Self> {
        let traces = config
            .traces
            .map(|protocol| {
                opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_protocol(protocol)
                    .build()
            })
            .transpose()
            .map_err(|_| error("telemetry_config", "Unable to configure trace exporter"))?;
        let metrics = config
            .metrics
            .map(|protocol| {
                opentelemetry_otlp::MetricExporter::builder()
                    .with_http()
                    .with_protocol(protocol)
                    .build()
            })
            .transpose()
            .map_err(|_| error("telemetry_config", "Unable to configure metric exporter"))?;
        let logs = config
            .logs
            .map(|protocol| {
                opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .with_protocol(protocol)
                    .build()
            })
            .transpose()
            .map_err(|_| error("telemetry_config", "Unable to configure log exporter"))?;
        // The SDK resource builder includes OTEL_RESOURCE_ATTRIBUTES and
        // OTEL_SERVICE_NAME. Explicit service version belongs to the executable.
        let resource = Resource::builder()
            .with_service_name(env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| service.to_owned()))
            .with_attribute(KeyValue::new("service.version", version.to_owned()));
        let resource = if let Ok(application_id) = env::var("OTEL_APPLICATION_ID") {
            resource.with_attribute(KeyValue::new("application.id", application_id))
        } else {
            resource
        }
        .build();
        let runtime = Self {
            traces: traces.map(|exporter| {
                SdkTracerProvider::builder()
                    .with_resource(resource.clone())
                    .with_batch_exporter(exporter)
                    .build()
            }),
            metrics: metrics.map(|exporter| {
                SdkMeterProvider::builder()
                    .with_resource(resource.clone())
                    .with_periodic_exporter(exporter)
                    .build()
            }),
            logs: logs.map(|exporter| {
                SdkLoggerProvider::builder()
                    .with_resource(resource)
                    .with_batch_exporter(exporter)
                    .build()
            }),
        };
        let bridge = runtime.logs.as_ref().map(OpenTelemetryTracingBridge::new);
        // Exporter/SDK diagnostics must not feed their own log exporter.
        let application_logs = tracing_subscriber::filter::filter_fn(|metadata| {
            !["opentelemetry", "reqwest", "hyper", "h2", "rustls"]
                .iter()
                .any(|prefix| metadata.target().starts_with(prefix))
        });
        let installed = tracing_subscriber::registry()
            .with(filter)
            .with(application_logs)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_writer(std::io::stderr),
            )
            .with(bridge)
            .try_init();
        if installed.is_err() {
            let _ = runtime.stop();
            return Err(error(
                "telemetry_installed",
                "A tracing subscriber is already installed",
            ));
        }
        if let Some(provider) = &runtime.traces {
            opentelemetry::global::set_tracer_provider(provider.clone());
        }
        if let Some(provider) = &runtime.metrics {
            opentelemetry::global::set_meter_provider(provider.clone());
        }
        config.install_propagator();
        Ok(runtime)
    }

    /// Flushes and shuts down all providers, attempting every signal even if one
    /// fails. Call after requests/handlers drain and before the Tokio runtime exits.
    ///
    /// # Errors
    /// Returns a sanitized error if any signal cannot flush/shut down.
    pub async fn shutdown(self) -> FrameworkResult<()> {
        tokio::task::spawn_blocking(move || self.stop())
            .await
            .map_err(|_| error("telemetry_shutdown", "Telemetry shutdown failed"))?
    }

    fn stop(self) -> FrameworkResult<()> {
        let mut failed = false;
        if let Some(provider) = self.traces {
            failed |= provider.shutdown().is_err();
        }
        if let Some(provider) = self.metrics {
            failed |= provider.shutdown().is_err();
        }
        if let Some(provider) = self.logs {
            failed |= provider.shutdown().is_err();
        }
        if failed {
            Err(error(
                "telemetry_shutdown",
                "Unable to flush all telemetry signals",
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
struct Configuration {
    trace_context: bool,
    baggage: bool,
    traces: Option<Protocol>,
    metrics: Option<Protocol>,
    logs: Option<Protocol>,
}

impl Configuration {
    fn install_propagator(self) {
        let mut propagators: Vec<
            Box<dyn opentelemetry::propagation::TextMapPropagator + Send + Sync>,
        > = Vec::new();
        if self.trace_context {
            propagators.push(Box::new(
                opentelemetry_sdk::propagation::TraceContextPropagator::new(),
            ));
        }
        if self.baggage {
            propagators.push(Box::new(
                opentelemetry_sdk::propagation::BaggagePropagator::new(),
            ));
        }
        opentelemetry::global::set_text_map_propagator(
            opentelemetry::propagation::TextMapCompositePropagator::new(propagators),
        );
    }

    fn from_env() -> FrameworkResult<Self> {
        Self::read(|name| env::var(name).ok())
    }

    fn read(get: impl Fn(&str) -> Option<String>) -> FrameworkResult<Self> {
        let disabled = match get("OTEL_SDK_DISABLED").as_deref() {
            None | Some("false" | "FALSE" | "False" | "") => false,
            Some("true" | "TRUE" | "True") => true,
            _ => return Err(error("telemetry_config", "Invalid OTEL_SDK_DISABLED")),
        };
        let signal = |name: &str| -> FrameworkResult<Option<Protocol>> {
            if disabled {
                return Ok(None);
            }
            let exporter = get(&format!("OTEL_{name}_EXPORTER"));
            match exporter.as_deref() {
                Some("none") => return Ok(None),
                None | Some("otlp") => {}
                _ => {
                    return Err(error(
                        "telemetry_config",
                        "Only OTLP or none exporters are supported",
                    ));
                }
            }
            let enabled = exporter.is_some()
                || get("OTEL_EXPORTER_OTLP_ENDPOINT").is_some()
                || get(&format!("OTEL_EXPORTER_OTLP_{name}_ENDPOINT")).is_some();
            if !enabled {
                return Ok(None);
            }
            let protocol = get(&format!("OTEL_EXPORTER_OTLP_{name}_PROTOCOL"))
                .or_else(|| get("OTEL_EXPORTER_OTLP_PROTOCOL"));
            match protocol.as_deref() {
                None | Some("http/protobuf") => Ok(Some(Protocol::HttpBinary)),
                Some("http/json") => Ok(Some(Protocol::HttpJson)),
                _ => Err(error(
                    "telemetry_config",
                    "Supported OTLP protocols are http/protobuf and http/json",
                )),
            }
        };
        let mut trace_context = false;
        let mut baggage = false;
        let propagation =
            get("OTEL_PROPAGATORS").unwrap_or_else(|| "tracecontext,baggage".to_owned());
        if propagation != "none" {
            for name in propagation.split(',').map(str::trim) {
                match name {
                    "tracecontext" => trace_context = true,
                    "baggage" => baggage = true,
                    _ => return Err(error("telemetry_config", "Unsupported OTEL_PROPAGATORS")),
                }
            }
        }
        Ok(Self {
            trace_context,
            baggage,
            traces: signal("TRACES")?,
            metrics: signal("METRICS")?,
            logs: signal("LOGS")?,
        })
    }
}

fn error(code: &'static str, message: &'static str) -> FrameworkError {
    FrameworkError::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn configuration_is_opt_in_and_checks_each_enabled_signal() {
        let mut values = BTreeMap::<&str, &str>::new();
        let read = |values: &BTreeMap<&str, &str>| {
            Configuration::read(|name| values.get(name).map(ToString::to_string))
        };
        let config = read(&values).unwrap();
        assert!(config.traces.is_none() && config.metrics.is_none() && config.logs.is_none());
        values.insert("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4318");
        let config = read(&values).unwrap();
        assert!(config.traces.is_some() && config.metrics.is_some() && config.logs.is_some());
        values.insert("OTEL_LOGS_EXPORTER", "none");
        assert!(read(&values).unwrap().logs.is_none());
        values.insert("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "grpc");
        assert!(read(&values).is_err());
        values.insert("OTEL_SDK_DISABLED", "true");
        let config = read(&values).unwrap();
        assert!(config.traces.is_none() && config.metrics.is_none() && config.logs.is_none());
    }
}
