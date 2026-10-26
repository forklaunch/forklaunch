//! Explicit missing versioned default strings, with original schema callbacks.
use axum::{Extension, extract::Path, response::IntoResponse};
use forklaunch_axum::{CorrelationIdFormat, ForkLaunchRouter, ResponseValidators, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    FieldContract, RequestContract, ResponseContract, RouteContract, ValidationMode,
    VersionedContract,
};
use forklaunch_core::http::{
    DefaultErrorResponses, FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion,
};
use serde_json::json;
use std::time::Duration;
fn contract(surface: &str, name: &str, mode: ValidationMode) -> RouteContract {
    let mut route = RouteContract::get(
        format!("{surface}{name}"),
        "Defaults",
        format!("/{name}/:status/:body"),
    );
    route.options.response_validation = mode;
    for (version, status) in [("first", 200), ("last", 201)] {
        let mut field = FieldContract::required("match", "Match");
        field.schema = field
            .schema
            .with_json_schema(json!({"enum":["both",version]}).to_string());
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract {
                query: vec![field],
                ..RequestContract::default()
            },
            responses: vec![ResponseContract::new(status, None)],
        });
    }
    route
}
fn response(status: u16, body: &str, selected: &str) -> FrameworkResponse {
    let response = match body {
        "empty" => FrameworkResponse::new(status),
        "jsonstring" => FrameworkResponse::json(status, "\"message\""),
        "object" => FrameworkResponse::json(status, "{\"secret\":\"DO_NOT_LEAK\"}"),
        _ => FrameworkResponse::text(status, "message"),
    };
    response.with_header("x-selected", selected)
}
async fn handler(
    Path((status, body)): Path<(u16, String)>,
    Extension(selected): Extension<SelectedVersion>,
) -> impl IntoResponse {
    let response = response(status, &body, &selected.version);
    (
        axum::http::StatusCode::from_u16(response.status_code).unwrap(),
        response
            .headers
            .into_iter()
            .map(|(key, value)| {
                (
                    axum::http::HeaderName::from_bytes(key.as_bytes()).unwrap(),
                    axum::http::HeaderValue::from_str(&value).unwrap(),
                )
            })
            .collect::<axum::http::HeaderMap>(),
        response.body.as_text().unwrap_or_default().to_owned(),
    )
}
#[tokio::main]
async fn main() {
    let mut native = ForkLaunchRouter::new(ServiceInfo::new("native", "1"));
    let mut local = LocalRouter::new();
    let mut contracts = Vec::new();
    let mut profiles = DefaultErrorResponses::default();
    let mut validators = ResponseValidators::default();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        let route = contract("native", name, mode);
        profiles
            .insert_versioned(&route, &[401, 403, 404, 500], 4096, Duration::from_secs(1))
            .unwrap();
        validators
            .insert_versioned(
                &route,
                4096,
                Duration::from_secs(1),
                |_, response, matched| {
                    assert!(
                        matched
                            .branches
                            .iter()
                            .all(|branch| branch.responses.len() == 1
                                && [200, 201].contains(&branch.responses[0].status_code))
                    );
                    assert!(response.headers.contains_key("x-selected"));
                    None
                },
            )
            .unwrap();
        native = native.get(route, handler);
        let route = contract("local", name, mode);
        profiles
            .insert_versioned(&route, &[401, 403, 404, 500], 4096, Duration::from_secs(1))
            .unwrap();
        validators
            .insert_versioned(
                &route,
                4096,
                Duration::from_secs(1),
                |_, response, matched| {
                    assert!(
                        matched
                            .branches
                            .iter()
                            .all(|branch| branch.responses.len() == 1
                                && [200, 201].contains(&branch.responses[0].status_code))
                    );
                    assert!(response.headers.contains_key("x-selected"));
                    None
                },
            )
            .unwrap();
        contracts.push(route.clone());
        local.register(route, |request: FrameworkRequest| {
            Ok(response(
                request.params["status"].parse().unwrap(),
                &request.params["body"],
                &request.version.as_ref().unwrap().version,
            ))
        });
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let app = ForkLaunchRouter::new(ServiceInfo::new("defaults", "1"))
        .nest("/native", native)
        .nest("/local", local)
        .with_config(RouterConfig {
            default_error_responses: profiles,
            response_validators: validators,
            auth_error_format: forklaunch_axum::AuthErrorFormat::TypeScriptText,
            correlation_id_format: CorrelationIdFormat::TypeScript,
            ..RouterConfig::default()
        })
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
