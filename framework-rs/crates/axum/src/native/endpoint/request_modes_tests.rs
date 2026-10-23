use super::*;
use forklaunch_core::contract::{
    HttpMethod, ValidationMode,
    endpoint::{Fields, VersionedEndpoint},
};
use tower::ServiceExt;
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Query {
    #[schemars(length(min = 10))]
    value: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct NumericQuery {
    #[schemars(range(min = 10))]
    value: u16,
}
fn endpoint<Q: schemars::JsonSchema + Send + 'static>(
    mode: ValidationMode,
) -> Endpoint<Fields<NoBody, Q>, TextBody> {
    let base = Endpoint::<Fields<NoBody, Q>, TextBody>::new(
        "requestModes",
        "requestModes",
        HttpMethod::Get,
        "/modes",
        200,
    )
    .unwrap();
    let mut route = base.contract();
    route.options.request_validation = mode;
    Endpoint::from_contract(route).unwrap()
}
fn registry(route: RouteContract) -> ContractRegistry {
    let mut r = ContractRegistry::new();
    r.register(route);
    r
}
async fn send(app: Router, query: &str) -> u16 {
    app.oneshot(
        Request::builder()
            .uri(format!("/modes?value={query}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
    .status()
    .as_u16()
}
#[tokio::test]
async fn request_modes_fallback_only_to_representable_raw_fields_and_preserve_valid_coercion() {
    for mode in [
        ValidationMode::Error,
        ValidationMode::Warning,
        ValidationMode::None,
    ] {
        let descriptor = endpoint::<Query>(mode);
        let app = ContractRouter::<()>::new(registry(descriptor.contract()))
            .bind(descriptor, |_, input| async move { Ok(input.query.value) })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(send(app.clone(), "long-valid-value").await, 200);
        assert_eq!(
            send(app, "short").await,
            if mode == ValidationMode::Error {
                400
            } else {
                200
            }
        );
        let descriptor = endpoint::<NumericQuery>(mode);
        let app = ContractRouter::<()>::new(registry(descriptor.contract()))
            .bind(descriptor, |_, input| async move {
                Ok(input.query.value.to_string())
            })
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(send(app.clone(), "12").await, 200);
        assert_eq!(send(app, "5").await, 400);
    }
}
#[tokio::test]
async fn request_modes_do_not_relax_version_selection() {
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let typed = endpoint::<Query>(mode);
        let descriptor = VersionedEndpoint::new("one", &typed).unwrap();
        let handlers = VersionedHandlers::new(descriptor.clone())
            .bind("one", typed, |_, (), input| async { Ok(input.query.value) })
            .unwrap();
        let app = ContractRouter::<()>::new(registry(descriptor.contract()))
            .bind_versioned(handlers)
            .unwrap()
            .finish()
            .unwrap();
        assert_eq!(send(app, "short").await, 400);
    }
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct Payload {
    #[schemars(length(min = 10))]
    value: String,
}
#[tokio::test]
async fn request_modes_allow_body_assertions_but_never_operational_failures() {
    use forklaunch_core::contract::schema_validation::SchemaValidationFailure;
    for mode in [ValidationMode::Warning, ValidationMode::None] {
        let base = Endpoint::<JsonBody<Payload>, TextBody>::new(
            "body",
            "body",
            HttpMethod::Post,
            "/modes",
            200,
        )
        .unwrap();
        let mut route = base.contract();
        route.options.request_validation = mode;
        let mut plan = RequestPlan::new(&route).unwrap();
        let (mut parts, _) = Request::builder()
            .method("POST")
            .uri("/modes")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap()
            .into_parts();
        let body = br#"{"value":"short"}"#;
        assert_eq!(
            plan.decode::<JsonBody<Payload>, ()>(&mut parts, &(), body)
                .await
                .unwrap()
                .value,
            "short"
        );
        plan.validator = Some(Arc::new(|_| Err(SchemaValidationFailure::Operational)));
        assert!(
            plan.decode::<JsonBody<Payload>, ()>(&mut parts, &(), body)
                .await
                .is_err()
        );
    }
}

#[test]
fn request_modes_warning_is_redacted() {
    if super::response_modes_tests::isolated_capture_test() {
        return;
    }
    let output = super::response_modes_tests::capture_warnings(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let endpoint = endpoint::<Query>(ValidationMode::Warning);
            let plan = RequestPlan::new(&endpoint.contract()).unwrap();
            let (mut parts, _) = Request::builder()
                .uri("/modes?value=secret")
                .body(Body::empty())
                .unwrap()
                .into_parts();
            let value = plan
                .decode::<Fields<NoBody, Query>, ()>(&mut parts, &(), &[])
                .await
                .unwrap();
            assert_eq!(value.query.value, "secret");
        });
    });
    assert!(output.contains("schema_rejection"));
    assert!(!output.contains("secret"));
}
