//! Shared HTTP/MCP security, listener lifecycle and process telemetry composition.
use super::{NativeHttpPolicy, export_contracts_from_env, secure, with_telemetry};
use axum::Router;
use forklaunch_core::contract::ContractRegistry;
use forklaunch_core::mcp::server::{McpServerOptions, mount_http, serve_http};
use forklaunch_core::telemetry::{OpenTelemetryCollector, otlp::TelemetryRuntime};
use std::{error::Error, future::Future};

pub type ApplicationResult<T = ()> = Result<T, Box<dyn Error>>;

/// Finish application work before releasing its resources, on success or error.
/// Bootstrap code supplies the service-owned cleanup; HTTP and MCP serving
/// remain inside the framework boundary. Cancellation or panic of the entire
/// bootstrap future does not run asynchronous cleanup.
///
/// # Errors
/// Returns the application's error after cleanup has completed.
pub async fn with_cleanup<T>(
    application: impl Future<Output = ApplicationResult<T>>,
    cleanup: impl Future<Output = ()>,
) -> ApplicationResult<T> {
    let result = application.await;
    cleanup.await;
    result
}

/// Application-owned routes and state enter the common security/transport boundary here.
/// The framework applies the same origin and telemetry policies to HTTP and MCP.
pub struct NativeApplication {
    router: Router,
    registry: ContractRegistry,
    options: McpServerOptions,
    policy: NativeHttpPolicy,
    telemetry: OpenTelemetryCollector,
}
impl NativeApplication {
    /// Construct the common listener composition; invalid origins fail before serving.
    ///
    /// # Errors
    /// Returns an error for invalid origins or incompatible managed response headers.
    pub fn new(
        router: Router,
        registry: ContractRegistry,
        service: &str,
        version: &str,
        origin: &str,
        telemetry: OpenTelemetryCollector,
    ) -> ApplicationResult<Self> {
        let policy = NativeHttpPolicy::new(origin)?;
        policy.validate_contracts(&registry)?;
        let mut options = McpServerOptions::new(service, version);
        options.transport = options.transport.with_allowed_origins([origin.to_owned()]);
        Ok(Self {
            router,
            registry,
            options,
            policy,
            telemetry,
        })
    }
    /// Allow these exact browser origins to call the HTTP routes cross-origin
    /// (see [`NativeHttpPolicy::with_cors_origins`]). Other origins stay denied.
    ///
    /// # Errors
    /// Rejects wildcards and malformed origins.
    pub fn with_cors_origins<I, O>(mut self, origins: I) -> ApplicationResult<Self>
    where
        I: IntoIterator<Item = O>,
        O: AsRef<str>,
    {
        self.policy = self.policy.with_cors_origins(origins)?;
        Ok(self)
    }
    /// Build an in-process router with MCP mounted using its default options.
    /// Executable MCP environment settings are interpreted by [`Self::serve`].
    ///
    /// # Errors
    /// Returns an error if MCP contracts cannot be compiled.
    pub fn router(self) -> ApplicationResult<Router> {
        let router = mount_http(self.router, &self.registry, self.options)?;
        Ok(with_telemetry(secure(router, self.policy), self.telemetry))
    }
    /// Serve configured shared/separate HTTP and MCP listeners, including atomic
    /// binding failures and bounded graceful shutdown.
    ///
    /// # Errors
    /// Returns configuration, listener or shutdown errors without request payloads.
    pub async fn serve<F>(self, listener: tokio::net::TcpListener, shutdown: F) -> ApplicationResult
    where
        F: Future<Output = ()> + Send,
    {
        serve_http(
            self.router,
            &self.registry,
            self.options,
            listener,
            shutdown,
            move |router| {
                with_telemetry(secure(router, self.policy.clone()), self.telemetry.clone())
            },
        )
        .await?;
        Ok(())
    }
}

/// Run application-owned startup under contract export and OTLP lifecycle policy.
/// The startup future is never polled during contract export. Telemetry is flushed
/// after successful startup/shutdown and after application errors.
///
/// # Errors
/// Returns export, startup, application or telemetry shutdown errors.
pub async fn run_application<F>(
    service: &str,
    version: &str,
    registry: &ContractRegistry,
    application: F,
) -> ApplicationResult
where
    F: Future<Output = ApplicationResult>,
{
    if export_contracts_from_env(registry)? {
        return Ok(());
    }
    let telemetry = TelemetryRuntime::start(service, version).await?;
    let result = application.await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;
    #[tokio::test]
    async fn resource_cleanup_finishes_after_application_success_and_failure() {
        use std::sync::Mutex;
        for succeeds in [true, false] {
            let order = Mutex::new(Vec::new());
            let result = with_cleanup(
                async {
                    order.lock().unwrap().push("application");
                    if succeeds {
                        Ok(7)
                    } else {
                        Err("startup failed".into())
                    }
                },
                async {
                    order.lock().unwrap().push("cleanup");
                },
            )
            .await;
            assert_eq!(*order.lock().unwrap(), ["application", "cleanup"]);
            if succeeds {
                assert_eq!(result.unwrap(), 7);
            } else {
                assert_eq!(result.unwrap_err().to_string(), "startup failed");
            }
        }
    }
    #[tokio::test]
    async fn common_application_keeps_origin_policy_on_app_routes() {
        let router = NativeApplication::new(
            Router::new().route("/health", get(|| async { "ready" })),
            ContractRegistry::default(),
            "test",
            "1",
            "https://example.test",
            OpenTelemetryCollector::new("test"),
        )
        .unwrap()
        .router()
        .unwrap();
        assert_eq!(
            router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            router
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .header("origin", "https://evil.test")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert!(
            NativeApplication::new(
                Router::new(),
                ContractRegistry::default(),
                "test",
                "1",
                "https://example.test/path",
                OpenTelemetryCollector::new("test")
            )
            .is_err()
        );
    }
    #[test]
    fn application_rejects_conflicting_security_header_contracts() {
        use forklaunch_core::contract::{
            HttpMethod,
            endpoint::{Endpoint, NoBody},
        };
        for (declared, valid) in [("no-cache", false), ("private, no-store", true)] {
            let endpoint =
                Endpoint::<NoBody, NoBody>::new("test", "Test", HttpMethod::Get, "/test", 204)
                    .unwrap()
                    .with_response_header("cache-control", declared)
                    .unwrap();
            let mut registry = ContractRegistry::default();
            registry.register(endpoint.contract());
            let result = NativeApplication::new(
                Router::new(),
                registry,
                "test",
                "1",
                "https://example.test",
                OpenTelemetryCollector::new("test"),
            );
            assert_eq!(result.is_ok(), valid);
        }
    }
}
