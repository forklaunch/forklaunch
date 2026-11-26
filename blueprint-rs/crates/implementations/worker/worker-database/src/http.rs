//! Optional health and bearer-authenticated local or distributed worker controls.
use crate::{
    control::{WorkerControl, WorkerState},
    retention::{self, RetainedRunReport, RetentionPolicy},
    runner::{self, JobHandler, RunnerConfig},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{AsyncWorkerAdmin, DistributedSignal};
use serde_json::{Value, json};
use std::{
    future::{Future, IntoFuture},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::watch;

/// No HTTP listener is created unless this configuration is supplied.
/// A missing token enables health/readiness only. Keep the configured secret out
/// of logs; this type intentionally does not implement Debug.
pub struct HttpConfig {
    pub address: SocketAddr,
    pub control_token: Option<String>,
}
impl HttpConfig {
    /// # Errors
    /// Rejects empty, short, oversized, or whitespace-bearing control tokens.
    pub fn validate(&self) -> BlueprintResult<()> {
        if self.control_token.as_ref().is_some_and(|token| {
            !(32..=4096).contains(&token.len())
                || token.bytes().any(|byte| !byte.is_ascii_graphic())
        }) {
            return Err(BlueprintError::new(
                "worker_invalid_http_config",
                "Worker control token must contain 32..=4096 printable ASCII characters without whitespace",
            ));
        }
        Ok(())
    }
}
/// Publishes a scoped command. The returned subscriber count is not an acknowledgement.
pub type SignalPublisher = Arc<
    dyn Fn(DistributedSignal) -> Pin<Box<dyn Future<Output = BlueprintResult<usize>> + Send>>
        + Send
        + Sync,
>;
#[derive(Clone)]
struct HttpState {
    control: WorkerControl,
    token: Option<Arc<str>>,
    publisher: Option<SignalPublisher>,
}
fn snapshot(control: &WorkerControl) -> Value {
    json!({"state":match control.state() {
        WorkerState::Running => "running",
        WorkerState::Paused => "paused",
        WorkerState::Draining => "draining",
    }})
}
async fn health(State(state): State<HttpState>) -> Json<Value> {
    Json(snapshot(&state.control))
}
async fn ready(State(state): State<HttpState>) -> Response {
    let status = if state.control.state() == WorkerState::Running {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(snapshot(&state.control))).into_response()
}
async fn command(
    State(state): State<HttpState>,
    Path(action): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = state.token else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let values = headers.get_all(header::AUTHORIZATION);
    let mut values = values.iter();
    let bearer = values
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if values.next().is_some()
        || !bearer.is_some_and(|value| bool::from(value.as_bytes().ct_eq(token.as_bytes())))
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    let signal = match action.as_str() {
        "pause" => DistributedSignal::Pause,
        "resume" => DistributedSignal::Resume,
        "drain" => DistributedSignal::Drain,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if state.control.state() == WorkerState::Draining && signal != DistributedSignal::Drain {
        return (StatusCode::CONFLICT, Json(snapshot(&state.control))).into_response();
    }
    if let Some(publish) = state.publisher {
        // Apply through the ordered subscription only. Applying locally again
        // after publication could overwrite a newer command already received.
        return match publish(signal).await {
            Ok(subscribers) => (
                StatusCode::ACCEPTED,
                Json(json!({"subscribers": subscribers})),
            )
                .into_response(),
            Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    }
    let accepted = if signal == DistributedSignal::Drain {
        state.control.drain();
        true
    } else {
        state.control.apply_signal(signal)
    };
    (
        if accepted {
            StatusCode::ACCEPTED
        } else {
            StatusCode::CONFLICT
        },
        Json(snapshot(&state.control)),
    )
        .into_response()
}
/// Builds internal health/readiness/control routes for one process. Readiness
/// describes consumer state, not database connectivity. No payloads are exposed.
/// # Errors
/// Rejects invalid control token configuration.
pub fn router(control: &WorkerControl, config: &HttpConfig) -> BlueprintResult<Router> {
    router_with_publisher(control, config, None)
}
/// Builds authenticated controls that optionally publish to a scoped worker group.
/// Publication failures return 503. Subscription delivery can still occur after
/// an ambiguous publication timeout. Commands apply through the subscription.
/// # Errors
/// Rejects invalid control token configuration.
pub fn router_with_publisher(
    control: &WorkerControl,
    config: &HttpConfig,
    publisher: Option<SignalPublisher>,
) -> BlueprintResult<Router> {
    config.validate()?;
    Ok(Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/control/{action}", post(command))
        .with_state(HttpState {
            control: control.clone(),
            token: config.control_token.as_deref().map(Arc::from),
            publisher,
        }))
}
fn server_error() -> BlueprintError {
    BlueprintError::new("worker_http_error", "Worker HTTP server failed")
}

/// Serve management HTTP without creating consumers, leases, or retention jobs.
/// Readiness describes this control process, not remote consumers or their queue.
/// Authenticated controls require a distributed publisher; they never mutate a
/// local consumer. A tokenless listener exposes only health/readiness.
/// # Errors
/// Rejects controls without a publisher, invalid config, and bind/server errors.
pub async fn serve_management<F>(
    config: &HttpConfig,
    publisher: Option<SignalPublisher>,
    shutdown: F,
) -> BlueprintResult<()>
where
    F: Future<Output = ()> + Send,
{
    config.validate()?;
    if config.control_token.is_some() && publisher.is_none() {
        return Err(BlueprintError::new(
            "worker_management_publisher_required",
            "Management controls require a distributed signal publisher",
        ));
    }
    let control = WorkerControl::new();
    let lifecycle = async {
        loop {
            let state = control.state();
            if state == WorkerState::Draining {
                return Ok(());
            }
            control.changed(state).await;
        }
    };
    serve_controlled(&control, Some(config), publisher, lifecycle, shutdown).await
}

/// Runs optional HTTP alongside controlled consumers and retention. The listener
/// stays available during handler drain, with readiness returning 503. Server
/// errors stop claims and drain work; HTTP shutdown is bounded to five seconds.
/// # Errors
/// Returns configuration, bind/server, queue, or retention failures after drain.
pub async fn run<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    policy: Option<&RetentionPolicy>,
    http: Option<&HttpConfig>,
    shutdown: F,
) -> BlueprintResult<RetainedRunReport>
where
    Q: AsyncWorkerAdmin,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    run_with_control(
        queue,
        handler,
        config,
        policy,
        http,
        &WorkerControl::new(),
        shutdown,
    )
    .await
}

/// Runs worker, retention, and HTTP management with a shared caller-owned control.
/// Distributed signal forwarding can use this same control while HTTP commands
/// retain their existing process-local behavior. Drain remains irreversible.
/// # Errors
/// Returns configuration, worker, retention or HTTP-server errors after shutdown.
pub async fn run_with_control<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    policy: Option<&RetentionPolicy>,
    http: Option<&HttpConfig>,
    control: &WorkerControl,
    shutdown: F,
) -> BlueprintResult<RetainedRunReport>
where
    Q: AsyncWorkerAdmin,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    run_with_publisher(
        queue, handler, config, policy, http, control, None, shutdown,
    )
    .await
}

/// Runs with optional scoped distributed HTTP publication and a shared local control.
/// The caller must forward incoming distributed signals to this same control.
/// # Errors
/// Returns configuration, worker, retention or HTTP-server errors after drain.
#[allow(clippy::too_many_arguments)]
pub async fn run_with_publisher<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    policy: Option<&RetentionPolicy>,
    http: Option<&HttpConfig>,
    control: &WorkerControl,
    publisher: Option<SignalPublisher>,
    shutdown: F,
) -> BlueprintResult<RetainedRunReport>
where
    Q: AsyncWorkerAdmin,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    config.validate()?;
    if let Some(policy) = policy {
        policy.validate()?;
    }
    let work = async {
        if let Some(policy) = policy {
            retention::run_worker_controlled(
                queue,
                handler,
                config,
                policy,
                control,
                std::future::pending(),
            )
            .await
        } else {
            runner::run_controlled(queue, handler, config, control, std::future::pending())
                .await
                .map(|worker| RetainedRunReport {
                    worker,
                    ..RetainedRunReport::default()
                })
        }
    };
    serve_controlled(control, http, publisher, work, shutdown).await
}

/// Serves optional health and authenticated controls around any worker future.
/// The worker must observe the shared control and finish draining on shutdown.
/// HTTP remains available while work drains; server errors also initiate drain.
/// # Errors
/// Returns configuration, bind/server, or worker errors after graceful shutdown.
pub async fn serve_controlled<R, W, F>(
    control: &WorkerControl,
    http: Option<&HttpConfig>,
    publisher: Option<SignalPublisher>,
    work: W,
    shutdown: F,
) -> BlueprintResult<R>
where
    W: Future<Output = BlueprintResult<R>>,
    F: Future<Output = ()> + Send,
{
    let server = if let Some(http) = http {
        let router = router_with_publisher(control, http, publisher)?;
        let listener = tokio::net::TcpListener::bind(http.address)
            .await
            .map_err(|_| server_error())?;
        Some((listener, router))
    } else {
        None
    };
    let (stop, mut receiver) = watch::channel(false);
    let serving = async {
        if let Some((listener, router)) = server {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    if !*receiver.borrow() {
                        let _ = receiver.changed().await;
                    }
                })
                .into_future()
                .await
                .map_err(|_| server_error())
        } else {
            std::future::pending::<BlueprintResult<()>>().await
        }
    };
    tokio::pin!(serving, work, shutdown);
    let mut stopping = false;
    let mut server_done = false;
    let mut first_error = None;
    let result = loop {
        tokio::select! {
            biased;
            () = &mut shutdown, if !stopping => { stopping=true; control.drain(); }
            result = &mut serving, if !server_done => {
                server_done=true;
                stopping=true;
                control.drain();
                first_error=Some(result.err().unwrap_or_else(server_error));
            }
            result = &mut work => break result,
        }
    };
    stop.send_replace(true);
    if http.is_some() && !server_done {
        // Dropping the serve future after this deadline bounds persistent clients.
        if let Ok(Err(error)) = tokio::time::timeout(Duration::from_secs(5), &mut serving).await
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(result, Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    const TOKEN: &str = "test-only-control-token-with-32-characters";
    async fn request(app: &Router, method: &str, path: &str, token: Option<&str>) -> Response {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, token);
        }
        app.clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    fn config(token: Option<&str>) -> HttpConfig {
        HttpConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            control_token: token.map(str::to_owned),
        }
    }
    #[tokio::test]
    async fn management_requires_distributed_controls_and_shuts_down_without_work() {
        let error = serve_management(&config(Some(TOKEN)), None, std::future::pending())
            .await
            .unwrap_err();
        assert_eq!(error.code, "worker_management_publisher_required");
        tokio::time::timeout(
            Duration::from_secs(1),
            serve_management(&config(None), None, async {}),
        )
        .await
        .unwrap()
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut occupied = config(None);
        occupied.address = listener.local_addr().unwrap();
        assert!(
            serve_management(&occupied, None, std::future::pending())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn controls_require_explicit_bearer_and_drain_remains_irreversible() {
        let control = WorkerControl::new();
        let app = router(&control, &config(Some(TOKEN))).unwrap();
        for token in [
            None,
            Some("Bearer wrong"),
            Some(TOKEN),
            Some("Basic invalid"),
        ] {
            let response = request(&app, "POST", "/control/pause", token).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
            assert_eq!(control.state(), WorkerState::Running);
        }
        let auth = format!("Bearer {TOKEN}");
        for action in ["pause", "resume", "drain"] {
            assert_eq!(
                request(&app, "POST", &format!("/control/{action}"), Some(&auth))
                    .await
                    .status(),
                StatusCode::ACCEPTED
            );
            let ready = request(&app, "GET", "/ready", None).await;
            assert_eq!(
                ready.status(),
                if action == "resume" {
                    StatusCode::OK
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                }
            );
        }
        assert_eq!(
            request(&app, "POST", "/control/resume", Some(&auth))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            request(&app, "GET", "/control/resume", Some(&auth))
                .await
                .status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            request(&app, "POST", "/control/unknown", Some(&auth))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        let health = request(&app, "GET", "/health", None).await;
        assert_eq!(health.status(), StatusCode::OK);
        let body = to_bytes(health.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"state":"draining"})
        );
        assert!(!String::from_utf8_lossy(&body).contains(TOKEN));
    }
    #[tokio::test]
    async fn health_only_listener_disables_controls_and_duplicate_credentials_are_rejected() {
        let control = WorkerControl::new();
        let app = router(&control, &config(None)).unwrap();
        assert_eq!(
            request(&app, "GET", "/health", None).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            request(&app, "POST", "/control/drain", Some(TOKEN))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        let app = router(&control, &config(Some(TOKEN))).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/control/drain")
                    .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(control.state(), WorkerState::Running);
    }
    #[tokio::test]
    async fn distributed_controls_publish_only_authenticated_valid_commands() {
        let control = WorkerControl::new();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = received.clone();
        let publish: SignalPublisher = Arc::new(move |signal| {
            captured.lock().unwrap().push(signal);
            Box::pin(async { Ok(3) })
        });
        let app = router_with_publisher(&control, &config(Some(TOKEN)), Some(publish)).unwrap();
        let auth = format!("Bearer {TOKEN}");
        assert_eq!(
            request(&app, "POST", "/control/pause", None).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(&app, "POST", "/control/unknown", Some(&auth))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert!(received.lock().unwrap().is_empty());
        for action in ["pause", "resume", "drain"] {
            let response = request(&app, "POST", &format!("/control/{action}"), Some(&auth)).await;
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap(),
                json!({"subscribers":3})
            );
            // A publisher does not bypass ordered subscription delivery.
            assert_eq!(control.state(), WorkerState::Running);
        }
        control.drain();
        assert_eq!(
            request(&app, "POST", "/control/resume", Some(&auth))
                .await
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            *received.lock().unwrap(),
            vec![
                DistributedSignal::Pause,
                DistributedSignal::Resume,
                DistributedSignal::Drain
            ]
        );
    }

    #[tokio::test]
    async fn failed_publication_does_not_change_local_state_or_expose_storage_errors() {
        let control = WorkerControl::new();
        let publish: SignalPublisher = Arc::new(|_| {
            Box::pin(async { Err(BlueprintError::new("storage", "sensitive storage detail")) })
        });
        let app = router_with_publisher(&control, &config(Some(TOKEN)), Some(publish)).unwrap();
        let response = request(
            &app,
            "POST",
            "/control/drain",
            Some(&format!("Bearer {TOKEN}")),
        )
        .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(control.state(), WorkerState::Running);
    }

    #[test]
    fn rejects_unusable_tokens_without_echoing_secrets() {
        for token in [
            String::new(),
            "short".to_owned(),
            "x".repeat(4097),
            format!("{}\n", "x".repeat(32)),
            "é".repeat(32),
        ] {
            let error = config(Some(&token)).validate().unwrap_err();
            assert_eq!(error.code, "worker_invalid_http_config");
        }
        assert!(config(None).validate().is_ok());
        assert!(config(Some(TOKEN)).validate().is_ok());
    }
}
