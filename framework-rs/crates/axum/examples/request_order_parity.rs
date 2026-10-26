//! Actual-source request callback ordering comparison target.
use axum::Json;
use forklaunch_axum::{
    AuthErrorFormat, CorrelationIdFormat, ForkLaunchRouter, RequestValidationStage,
    RequestValidators, RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, BodyKind, FieldContract, ResponseContract, RouteContract, SchemaRef,
    ValidationMode,
};
use forklaunch_core::http::typescript_request_validation_error_response;
use forklaunch_core::http::{ErrorKind, ErrorResponse};
use forklaunch_validator::typescript_coerce;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[tokio::main]
async fn main() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut validators = RequestValidators::default();
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("request-order", "1"));
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        for auth in [false, true] {
            let mut route = RouteContract::get(
                format!("Validate{name}{auth}"),
                "Validate",
                format!("/validate/{name}/{auth}"),
            )
            .with_query(FieldContract::optional("limit", "Number"))
            .with_response(200, None);
            route.options.request_validation = mode;
            if auth {
                route.access = AccessLevel::Authenticated;
            }
            validators
                .insert_with_rejection(
                    &route,
                    ResponseContract::new(
                        400,
                        Some(BodyContract {
                            kind: BodyKind::Text,
                            schema: SchemaRef::named("String"),
                            content_type: Some("text/plain".into()),
                        }),
                    ),
                    |request, raw| {
                        let values: Vec<_> =
                            form_urlencoded::parse(raw.unwrap_or_default().as_bytes())
                                .filter(|(key, _)| key == "limit")
                                .map(|(_, value)| Value::String(value.into_owned()))
                                .collect();
                        let value = match values.len() {
                            0 => None,
                            1 => values.into_iter().next(),
                            _ => Some(Value::Array(values)),
                        };
                        typescript_coerce::number(value.as_ref())
                            .err()
                            .map(|mut error| {
                                error.path = vec!["query".into(), "limit".into()];
                                typescript_request_validation_error_response(
                                    Some(&[error]),
                                    Some(&request.context.correlation_id),
                                )
                            })
                    },
                )
                .unwrap();
            validators
                .set_stage(&route, RequestValidationStage::BeforeAuthentication)
                .unwrap();
            let count = calls.clone();
            router = router.get(route, move || async move {
                count.fetch_add(1, Ordering::SeqCst);
                "ok"
            });
        }
    }
    let app = router
        .get(
            RouteContract::get("Calls", "Calls", "/calls").with_response(200, None),
            move || async move { Json(json!({"handlerCalls":calls.load(Ordering::SeqCst)})) },
        )
        .with_config(
            RouterConfig {
                request_validators: validators,
                auth_error_format: AuthErrorFormat::TypeScriptText,
                correlation_id_format: CorrelationIdFormat::TypeScript,
                ..RouterConfig::default()
            }
            .with_response_authenticator(|_, _| async {
                Err(ErrorResponse::new(
                    ErrorKind::Unauthorized,
                    "Authorization token required.",
                ))
            }),
        )
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
