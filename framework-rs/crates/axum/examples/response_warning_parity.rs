//! Finite response schema used by the actual-source Warning oracle.
use axum::{Json, extract::Path, response::IntoResponse};
use forklaunch_axum::{CorrelationIdFormat, ForkLaunchRouter, ResponseValidators, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{BodyContract, RouteContract, ValidationMode};
use forklaunch_core::http::typescript_response_validation_error_response;
use forklaunch_validator::{ParseError, typescript_coerce};
use serde_json::{Value, json};
use std::time::Duration;
fn errors(value: &Value, path: &str) -> Option<Vec<ParseError>> {
    typescript_coerce::number(Some(value))
        .err()
        .map(|mut error| {
            error.path = vec![path.into()];
            vec![error]
        })
}
async fn handler(Path(kind): Path<String>) -> impl IntoResponse {
    let header = if kind.contains("header") { "bad" } else { "2" };
    let amount = if kind.contains("body") {
        "secret-invalid"
    } else {
        "2"
    };
    (
        [("x-count", header), ("x-preserved", "yes")],
        Json(json!({"amount":amount,"extra":"keep"})),
    )
}
#[tokio::main]
async fn main() {
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("response-warning", "1"));
    let mut validators = ResponseValidators::default();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        let mut route = RouteContract::get(
            format!("Response{name}"),
            "Response",
            format!("/{name}/:kind"),
        )
        .with_response(200, Some(BodyContract::json("Output")));
        route.options.request_validation = ValidationMode::None;
        route.options.response_validation = mode;
        validators
            .insert(&route, 1024, Duration::from_secs(1), |request, response| {
                let value: Value = serde_json::from_str(response.body.as_text().unwrap()).unwrap();
                let headers = errors(&json!(response.headers["x-count"]), "x-count");
                let body = errors(&value["amount"], "amount");
                typescript_response_validation_error_response(
                    headers.as_deref(),
                    body.as_deref(),
                    Some(&request.context.correlation_id),
                )
            })
            .unwrap();
        validators.enable_warning(&route).unwrap();
        router = router.get(route, handler);
    }
    let app = router
        .with_config(RouterConfig {
            response_validators: validators,
            correlation_id_format: CorrelationIdFormat::TypeScript,
            ..RouterConfig::default()
        })
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
