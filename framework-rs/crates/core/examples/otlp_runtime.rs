//! Process fixture for real OTLP receiver tests; no global environment mutation
//! or global subscriber installation takes place in the parallel unit test suite.
use forklaunch_core::telemetry::{OpenTelemetryCollector, otlp::TelemetryRuntime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let runtime = TelemetryRuntime::start("otlp-fixture", "1.2.3").await?;
            let collector = OpenTelemetryCollector::new("otlp-fixture");
            collector.start_request("GET", "/items/{id}").finish(
                200,
                Some(42),
                "fixture-correlation",
            );
            collector.info("fixture application log", "fixture-correlation");
            // A repeat attempt must not replace the process-global providers.
            assert!(TelemetryRuntime::start("replacement", "0").await.is_err());
            runtime.shutdown().await?;
            Ok(())
        })
}
