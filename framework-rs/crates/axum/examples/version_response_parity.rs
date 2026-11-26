//! Finite literal-union callback; not a generic response schema interpreter.
use axum::{Extension, extract::Path, http::StatusCode, response::IntoResponse};
use forklaunch_axum::{
    CorrelationIdFormat, ForkLaunchRouter, ResponseStatusErrorFormat, ResponseValidators,
    RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    BodyContract, BodyKind, FieldContract, RequestContract, ResponseContract, RouteContract,
    SchemaRef, ValidationMode, VersionedContract,
};
use forklaunch_core::http::typescript_response_validation_error_response;
use forklaunch_core::http::{
    FrameworkRequest, FrameworkResponse, LocalRouter, MatchedVersionResponses, SelectedVersion,
};
use forklaunch_validator::ParseError;
use serde_json::{Value, json};
use std::time::Duration;
fn contract(surface: &str, mode: ValidationMode, name: &str) -> RouteContract {
    let mut route = RouteContract::get(
        format!("{surface}{name}"),
        "Version response",
        format!("/{name}/:status/:body"),
    );
    route.options.response_validation = mode;
    for (version, matching, status, literal, shared) in [
        ("z-first", "first", 201, "first", "old"),
        ("a-last", "last", 202, "last", "new"),
    ] {
        let mut field = FieldContract::required("match", "Match");
        field.schema = field
            .schema
            .with_json_schema(json!({"enum":["both",matching]}).to_string());
        let responses = [(status, literal), (203, shared)]
            .into_iter()
            .map(|(status, literal)| {
                ResponseContract::new(
                    status,
                    Some(BodyContract {
                        kind: BodyKind::Text,
                        content_type: Some("text/plain".into()),
                        schema: SchemaRef::named("Literal")
                            .with_json_schema(json!({"const":literal}).to_string()),
                    }),
                )
            })
            .collect();
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract {
                query: vec![field],
                ..RequestContract::default()
            },
            responses,
        });
    }
    route
}
fn validate(
    request: &FrameworkRequest,
    response: &FrameworkResponse,
    matched: &MatchedVersionResponses,
) -> Option<FrameworkResponse> {
    if response.status_code == 500 {
        return None;
    }
    let literals: Vec<String> = matched
        .branches
        .iter()
        .flat_map(|branch| &branch.responses)
        .filter(|schema| schema.status_code == response.status_code)
        .map(|schema| {
            let value: Value = serde_json::from_str(
                schema
                    .body
                    .as_ref()
                    .unwrap()
                    .schema
                    .json_schema
                    .as_ref()
                    .unwrap(),
            )
            .unwrap();
            value["const"].as_str().unwrap().to_owned()
        })
        .collect();
    if literals
        .iter()
        .any(|literal| response.body.as_text() == Some(literal.as_str()))
    {
        return None;
    }
    let errors: Vec<_> = literals
        .iter()
        .enumerate()
        .map(|(index, literal)| ParseError {
            path: vec![format!("Union Schema Variant {index}")],
            message: format!("Invalid literal value, expected {literal:?}"),
        })
        .collect();
    typescript_response_validation_error_response(
        None,
        Some(&errors),
        Some(&request.context.correlation_id),
    )
}
fn origin_contract(surface: &str, protected: bool) -> RouteContract {
    let mut field = FieldContract::required("match", "Match");
    field.schema = field
        .schema
        .with_json_schema(json!({"const":"yes"}).to_string());
    let mut body = BodyContract::json("Origin");
    body.schema = body.schema.with_json_schema(
        json!({"type":"object","required":["flag"],"properties":{"flag":{"const":"ok"}}})
            .to_string(),
    );
    RouteContract::post(
        format!("{surface}Origin{protected}"),
        "Origin",
        format!("/origin/{protected}"),
    )
    .with_access(if protected {
        forklaunch_core::contract::AccessLevel::Authenticated
    } else {
        forklaunch_core::contract::AccessLevel::Public
    })
    .with_version(VersionedContract {
        version: "only".into(),
        request: RequestContract {
            query: vec![field],
            body: Some(body),
            ..RequestContract::default()
        },
        responses: vec![ResponseContract::new(
            200,
            Some(BodyContract::json("Output")),
        )],
    })
}
async fn handler(
    Path((status, body)): Path<(u16, String)>,
    Extension(selected): Extension<SelectedVersion>,
) -> impl IntoResponse {
    (
        StatusCode::from_u16(status).unwrap(),
        [("x-selected", selected.version)],
        body,
    )
}
#[tokio::main]
async fn main() {
    let mut validators = ResponseValidators::default();
    let mut native = ForkLaunchRouter::new(ServiceInfo::new("native", "1"));
    let mut local = LocalRouter::new();
    let mut local_contracts = Vec::new();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        let route = contract("native", mode, name);
        validators
            .insert_versioned(&route, 4096, Duration::from_secs(1), validate)
            .unwrap();
        validators.enable_warning(&route).unwrap();
        native = native.get(route, handler);
        let route = contract("local", mode, name);
        validators
            .insert_versioned(&route, 4096, Duration::from_secs(1), validate)
            .unwrap();
        validators.enable_warning(&route).unwrap();
        local_contracts.push(route.clone());
        local.register(route, |request: FrameworkRequest| {
            Ok(FrameworkResponse::text(
                request.params["status"].parse().unwrap(),
                request.params["body"].clone(),
            )
            .with_header(
                "x-selected",
                request.version.as_ref().unwrap().version.clone(),
            ))
        });
    }
    for protected in [false, true] {
        let route = origin_contract("native", protected);
        validators
            .insert_versioned(&route, 4096, Duration::from_secs(1), |_, _, _| {
                Some(FrameworkResponse::text(500, "CALLBACK_MUST_NOT_RUN"))
            })
            .unwrap();
        native = native.post(route, || async { "ORIGIN_HANDLER_RAN" });
        let route = origin_contract("local", protected);
        validators
            .insert_versioned(&route, 4096, Duration::from_secs(1), |_, _, _| {
                Some(FrameworkResponse::text(500, "CALLBACK_MUST_NOT_RUN"))
            })
            .unwrap();
        local_contracts.push(route.clone());
        local.register(route, |_: FrameworkRequest| {
            Ok(FrameworkResponse::text(200, "ORIGIN_HANDLER_RAN"))
        });
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), local_contracts, local);
    let app = ForkLaunchRouter::new(ServiceInfo::new("union", "1"))
        .nest("/native", native)
        .nest("/local", local)
        .with_config(
            RouterConfig {
                response_validators: validators,
                correlation_id_format: CorrelationIdFormat::TypeScript,
                response_status_error_format: ResponseStatusErrorFormat::TypeScriptText,
                ..RouterConfig::default()
            }
            .with_response_authenticator(|_, _| async {
                Err(forklaunch_core::http::ErrorResponse::new(
                    forklaunch_core::http::ErrorKind::Unauthorized,
                    "missing token",
                ))
            }),
        )
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
