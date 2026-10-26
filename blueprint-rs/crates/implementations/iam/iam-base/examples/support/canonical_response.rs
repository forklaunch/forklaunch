//! Fixture schema and trusted public diagnostics; the registry owns enforcement.
use axum::response::IntoResponse;
use forklaunch_axum::{ForkLaunchRouter, ResponseValidators, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::http::{ParseError, typescript_response_validation_error_response};
use forklaunch_core::{
    contract::{BodyContract, BodyKind, FieldContract, RouteContract, SchemaRef},
    http::{FrameworkRequest, FrameworkResponse, LocalRouter},
};
use std::time::Duration;

pub fn router() -> ForkLaunchRouter {
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("response", "1"));
    let mut validators = ResponseValidators::default();
    for local in [false, true] {
        let path = if local { "/local/:mode" } else { "/axum/:mode" };
        let mut contract = RouteContract::get(
            if local {
                "Local Response"
            } else {
                "Axum Response"
            },
            "Response",
            path,
        )
        .with_param(FieldContract::required("mode", "String"))
        .with_response(
            200,
            Some(BodyContract {
                kind: BodyKind::Text,
                schema: SchemaRef::named("String"),
                content_type: Some("text/plain".into()),
            }),
        )
        .with_response_header(200, FieldContract::required("x-result", "String"))
        .with_response(500, None);
        contract.options.openapi = false;
        validators
            .insert(&contract, 1024, Duration::from_secs(1), validate)
            .unwrap();
        if local {
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |request: FrameworkRequest| {
                Ok(response(&request.params["mode"]))
            });
            router = router.nest(
                "/",
                ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("local-response", "1"),
                    [contract],
                    handlers,
                ),
            );
        } else {
            router = router.get(
                contract,
                |axum::extract::Path(mode): axum::extract::Path<String>| async move {
                    let response = response(&mode);
                    let mut output = (
                        axum::http::StatusCode::from_u16(response.status_code).unwrap(),
                        response.body.as_text().unwrap_or_default().to_owned(),
                    )
                        .into_response();
                    output.headers_mut().clear();
                    for (name, value) in response.headers {
                        output.headers_mut().insert(
                            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                            value.parse().unwrap(),
                        );
                    }
                    output
                },
            );
        }
    }
    router.with_config(RouterConfig {
        response_validators: validators,
        ..RouterConfig::default()
    })
}

fn response(mode: &str) -> FrameworkResponse {
    if mode == "empty" {
        return FrameworkResponse::new(500);
    }
    let mut response = if matches!(mode, "wrong" | "both") {
        FrameworkResponse::json(200, "42")
    } else {
        FrameworkResponse::text(200, "ok")
    };
    if !matches!(mode, "headers" | "both") {
        response = response.with_header("x-result", "valid");
    }
    response
}

fn validate(request: &FrameworkRequest, response: &FrameworkResponse) -> Option<FrameworkResponse> {
    // Empty500.send() bypasses upstream response parsing; preserve that boundary.
    if response.body.is_empty() {
        return None;
    }
    let headers = (!response.headers.contains_key("x-result")).then(|| {
        vec![ParseError {
            path: vec!["x-result".into()],
            message: "Required".into(),
        }]
    });
    let body = (response
        .headers
        .get("content-type")
        .is_some_and(|value| value == "application/json"))
    .then(|| {
        vec![ParseError {
            path: vec![],
            message: "Expected string, received number".into(),
        }]
    });
    typescript_response_validation_error_response(
        headers.as_deref(),
        body.as_deref(),
        Some(&request.context.correlation_id),
    )
}
