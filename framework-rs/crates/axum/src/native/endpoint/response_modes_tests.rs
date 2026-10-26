use super::*;
use forklaunch_core::contract::{
    HttpMethod, ValidationMode,
    endpoint::{ResponseWithHeaders, VersionedEndpoint, WithResponseHeaders},
};
use tower::ServiceExt;

#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct Output {
    #[schemars(length(min = 10))]
    text: String,
}
#[derive(serde::Serialize, schemars::JsonSchema)]
struct Headers {
    #[schemars(length(min = 10))]
    #[serde(rename = "x-label")]
    label: String,
}
fn registry(route: RouteContract) -> ContractRegistry {
    let mut r = ContractRegistry::new();
    r.register(route);
    r
}
fn endpoint(
    mode: ValidationMode,
) -> Endpoint<NoBody, WithResponseHeaders<JsonBody<Output>, Headers>> {
    let endpoint = Endpoint::<NoBody, WithResponseHeaders<JsonBody<Output>, Headers>>::new(
        "modes",
        "modes",
        HttpMethod::Get,
        "/modes",
        200,
    )
    .unwrap();
    let mut route = endpoint.contract();
    route.options.response_validation = mode;
    Endpoint::from_contract(route).unwrap()
}
#[tokio::test]
async fn response_modes_apply_body_and_header_assertions_to_finite_and_versioned_handlers() {
    for mode in [
        ValidationMode::Error,
        ValidationMode::Warning,
        ValidationMode::None,
    ] {
        for versioned in [false, true] {
            let endpoint = endpoint(mode);
            let app = if versioned {
                let descriptor = VersionedEndpoint::new("one", &endpoint).unwrap();
                let handlers = versions::VersionedHandlers::new(descriptor.clone())
                    .bind("one", endpoint, |_, (), ()| async {
                        Ok(ResponseWithHeaders {
                            body: Output {
                                text: "canary".into(),
                            },
                            headers: Headers {
                                label: "short".into(),
                            },
                        })
                    })
                    .unwrap();
                ContractRouter::<()>::new(registry(descriptor.contract()))
                    .bind_versioned(handlers)
                    .unwrap()
                    .finish()
                    .unwrap()
            } else {
                ContractRouter::<()>::new(registry(endpoint.contract()))
                    .bind(endpoint, |_, ()| async {
                        Ok(ResponseWithHeaders {
                            body: Output {
                                text: "canary".into(),
                            },
                            headers: Headers {
                                label: "short".into(),
                            },
                        })
                    })
                    .unwrap()
                    .finish()
                    .unwrap()
            };
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/modes")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status().as_u16(),
                if mode == ValidationMode::Error {
                    500
                } else {
                    200
                }
            );
            if mode != ValidationMode::Error {
                assert_eq!(response.headers()["x-label"], "short");
            }
        }
    }
}

#[cfg(feature = "native-streams")]
#[tokio::test]
async fn response_modes_apply_to_each_sse_frame_without_weakening_framing() {
    use crate::native::{ServerEvent, Shutdown, StreamOptions};
    use forklaunch_core::contract::endpoint::SseBody;
    use std::time::Duration;
    for mode in [
        ValidationMode::Error,
        ValidationMode::Warning,
        ValidationMode::None,
    ] {
        for versioned in [false, true] {
            let base = Endpoint::<NoBody, SseBody<Output>>::new(
                "stream",
                "stream",
                HttpMethod::Get,
                "/modes",
                200,
            )
            .unwrap();
            let mut route = base.contract();
            route.options.response_validation = mode;
            let endpoint = Endpoint::<NoBody, SseBody<Output>>::from_contract(route).unwrap();
            let options = StreamOptions::new(
                Shutdown::new(),
                1024,
                Duration::from_millis(10),
                Duration::from_secs(1),
            )
            .unwrap();
            let app = if versioned {
                let descriptor = VersionedEndpoint::new("one", &endpoint).unwrap();
                let handlers =
                    crate::native::VersionedSseHandlers::new(descriptor.clone(), options)
                        .bind("one", endpoint, |_, _, ()| async {
                            Ok(futures_util::stream::iter([Ok(ServerEvent::new(Output {
                                text: "canary".into(),
                            }))]))
                        })
                        .unwrap();
                ContractRouter::<()>::new(registry(descriptor.contract()))
                    .bind_versioned_sse_authorized(handlers, Public)
                    .unwrap()
                    .finish()
                    .unwrap()
            } else {
                ContractRouter::<()>::new(registry(endpoint.contract()))
                    .bind_sse_authorized(endpoint, Public, options, |_, _, ()| async {
                        Ok(futures_util::stream::iter([Ok(ServerEvent::new(Output {
                            text: "canary".into(),
                        }))]))
                    })
                    .unwrap()
                    .finish()
                    .unwrap()
            };
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/modes")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
            assert_eq!(
                String::from_utf8_lossy(&bytes).contains("canary"),
                mode != ValidationMode::Error
            );
        }
    }
}

#[test]
fn response_modes_never_hide_malformed_wire_or_operational_validation_failure() {
    use forklaunch_core::contract::schema_validation::SchemaValidationFailure;
    for mode in [
        ValidationMode::Error,
        ValidationMode::Warning,
        ValidationMode::None,
    ] {
        let route = endpoint(mode).contract();
        let mut plans = responses::configure(responses::plans(&route.responses).unwrap(), &route);
        let plan = plans.get_mut(&(200, "application/json".into())).unwrap();
        assert!(
            plan.render(
                b"malformed-json".to_vec(),
                [("x-label".into(), "valid label".into())].into()
            )
            .is_err()
        );
        assert!(
            plan.render(
                br#"{"text":"valid body text"}"#.to_vec(),
                [("x-label".into(), "bad\r\nheader".into())].into()
            )
            .is_err()
        );
        if mode != ValidationMode::None {
            plan.validator = Some(Arc::new(|_| Err(SchemaValidationFailure::Operational)));
            assert!(
                plan.render(
                    br#"{"text":"valid body text"}"#.to_vec(),
                    [("x-label".into(), "valid label".into())].into()
                )
                .is_err()
            );
        }
    }
}

// Tracing caches callsite interest process-wide, while these capture subscribers
// are thread-local. Run each capture assertion alone so parallel router probes
// cannot register its first callsite under a different subscriber.
pub(super) fn isolated_capture_test() -> bool {
    const MARKER: &str = "FORKLAUNCH_WARNING_CAPTURE_TEST";
    let name = std::thread::current()
        .name()
        .expect("capture must run in a named test thread")
        .to_owned();
    if std::env::var(MARKER).ok().as_deref() == Some(name.as_str()) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1", "--nocapture"])
        .env(MARKER, &name)
        .output()
        .expect("launch isolated warning capture test");
    assert!(
        output.status.success(),
        "isolated warning capture failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

pub(super) fn capture_warnings(run: impl FnOnce()) -> String {
    use std::sync::Mutex;
    #[derive(Clone)]
    struct Capture(Arc<Mutex<String>>);
    impl tracing::Subscriber for Capture {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            struct Visitor<'a>(&'a mut String);
            impl tracing::field::Visit for Visitor<'_> {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    use std::fmt::Write;
                    let _ = write!(self.0, "{}={value:?};", field.name());
                }
            }
            event.record(&mut Visitor(&mut self.0.lock().unwrap()));
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }
    let output = Arc::new(Mutex::new(String::new()));
    tracing::subscriber::with_default(Capture(output.clone()), run);
    output.lock().unwrap().clone()
}
#[test]
fn response_modes_warning_emits_only_non_sensitive_metadata() {
    if isolated_capture_test() {
        return;
    }
    let output = capture_warnings(|| {
        let route = endpoint(ValidationMode::Warning).contract();
        let plans = responses::configure(responses::plans(&route.responses).unwrap(), &route);
        let plan = &plans[&(200, "application/json".into())];
        assert!(
            plan.render(
                br#"{"text":"canary"}"#.to_vec(),
                [("x-label".into(), "secret".into())].into()
            )
            .is_ok()
        );
    });
    assert!(output.contains("schema_rejection"));
    assert!(!output.contains("canary"));
    assert!(!output.contains("secret"));
}

#[forklaunch_core::contract::schema]
struct CheckedOutput {
    #[field(min_length = 10)]
    text: String,
}
#[tokio::test]
async fn response_modes_preserve_generated_value_codec_invariants() {
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let base = Endpoint::<NoBody, JsonBody<CheckedOutput>>::new(
            "checked",
            "checked",
            HttpMethod::Get,
            "/modes",
            200,
        )
        .unwrap();
        let mut route = base.contract();
        route.options.response_validation = mode;
        let endpoint = Endpoint::<NoBody, JsonBody<CheckedOutput>>::from_contract(route).unwrap();
        let app = ContractRouter::<()>::new(registry(endpoint.contract()))
            .bind(endpoint, |_, ()| async {
                Ok(CheckedOutput {
                    text: "invalid".into(),
                })
            })
            .unwrap()
            .finish()
            .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/modes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
    }
}
