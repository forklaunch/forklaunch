//! Finite caller-owned field/body schema, compared with actual TypeScript sources.
use axum::{
    Extension, Json,
    extract::{Path, RawQuery},
    http::HeaderMap,
};
use forklaunch_axum::{
    AuthErrorFormat, CorrelationIdFormat, ForkLaunchRouter, JsonRequestParsers, ParsedJsonBody,
    ParsedRequestFields, RequestFieldValues, RequestTransformOutput, RequestValidators,
    RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, BodyKind, FieldContract, ResponseContract, RouteContract, SchemaRef,
    ValidationMode,
};
use forklaunch_core::http::typescript_request_validation_error_response;
use forklaunch_core::http::{
    BodyParserHooks, ErrorKind, ErrorResponse, FrameworkRequest, FrameworkResponse,
    parse_typescript_simple_query,
};
use forklaunch_validator::{ParseError, typescript_coerce};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn number(value: Option<&Value>, path: &[&str], errors: &mut Vec<ParseError>) -> Option<Value> {
    typescript_coerce::number(value)
        .map(|number| json!(number))
        .map_err(|mut error| {
            error.path = path.iter().map(|value| (*value).into()).collect();
            errors.push(error);
        })
        .ok()
}
fn transform(
    request: &FrameworkRequest,
    raw: Option<&str>,
    mixed: bool,
) -> Result<RequestTransformOutput, FrameworkResponse> {
    let mut errors = Vec::new();
    let mut output = RequestTransformOutput::default();
    let params = request.params.get("id").map(|value| json!(value));
    if let Some(value) = number(params.as_ref(), &["params", "id"], &mut errors) {
        output.fields.params.insert("id".into(), value);
    }
    let query = parse_typescript_simple_query(raw);
    let limit = query.get("limit").cloned().unwrap_or_else(|| json!(7));
    if let Some(value) = number(Some(&limit), &["query", "limit"], &mut errors) {
        output.fields.query.insert("limit".into(), value);
    }
    let flag = request
        .headers
        .get("x-flag")
        .map_or_else(|| json!(true), |value| json!(value));
    match typescript_coerce::boolean(Some(&flag)) {
        Ok(flag) => {
            output.fields.headers.insert("x-flag".into(), json!(flag));
        }
        Err(mut error) => {
            error.path = vec!["headers".into(), "x-flag".into()];
            errors.push(error);
        }
    }
    let count = request
        .headers
        .get("x-count")
        .map_or_else(|| json!(5), |value| json!(value));
    if let Some(value) = number(Some(&count), &["headers", "x-count"], &mut errors) {
        output.fields.headers.insert("x-count".into(), value);
    }
    output.fields.headers.insert(
        "x-absent".into(),
        json!(
            request
                .headers
                .get("x-absent")
                .map_or("default", String::as_str)
        ),
    );
    if mixed {
        let body = request
            .parsed_json
            .as_ref()
            .and_then(|parsed| parsed.value.as_deref());
        if let Some(Value::Object(body)) = body {
            if let Some(amount) = number(body.get("amount"), &["body", "amount"], &mut errors) {
                output.body = Some(json!({"amount":amount}));
            }
        } else {
            errors.push(ParseError {
                path: vec!["body".into()],
                message: "Expected object, received null".into(),
            });
        }
    }
    if errors.is_empty() {
        Ok(output)
    } else {
        Err(typescript_request_validation_error_response(
            Some(&errors),
            Some(&request.context.correlation_id),
        ))
    }
}
async fn report(
    Path(params): Path<BTreeMap<String, String>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
    fields: Option<Extension<ParsedRequestFields>>,
    body: Option<Extension<ParsedJsonBody>>,
) -> Json<Value> {
    let values = fields.map_or_else(
        || RequestFieldValues {
            params: params
                .into_iter()
                .map(|(key, value)| (key, json!(value)))
                .collect(),
            query: parse_typescript_simple_query(raw.as_deref()),
            headers: headers
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (key.as_str().into(), json!(value)))
                })
                .collect(),
        },
        |Extension(fields)| fields.values().clone(),
    );
    let headers: BTreeMap<_, _> = values
        .headers
        .into_iter()
        .filter(|(key, _)| ["x-flag", "x-count", "x-absent", "x-extra"].contains(&key.as_str()))
        .collect();
    let mut result = json!({"params":values.params,"query":values.query,"headers":headers});
    if let Some(Extension(body)) = body {
        if let Some(value) = body.value {
            result["body"] = value.as_ref().clone();
        }
        if let Some(raw) = body.raw_body {
            result["raw"] = json!(String::from_utf8_lossy(&raw));
        }
    }
    Json(result)
}
fn rejection() -> ResponseContract {
    ResponseContract::new(
        400,
        Some(BodyContract {
            kind: BodyKind::Text,
            schema: SchemaRef::named("String"),
            content_type: Some("text/plain".into()),
        }),
    )
}
#[tokio::main]
async fn main() {
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("fields", "1"));
    let mut parsers = JsonRequestParsers::default();
    let mut validators = RequestValidators::default();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        for auth in [false, true] {
            for mixed in [false, true] {
                let prefix = if mixed { "mixed" } else { "fields" };
                let path = format!("/{prefix}/{name}/{auth}/:id/:unused");
                let operation = format!("Fields{name}{auth}{mixed}");
                let mut route = if mixed {
                    RouteContract::post(operation, "Fields", path)
                        .with_request_body(BodyContract::json("Input"))
                } else {
                    RouteContract::get(operation, "Fields", path)
                };
                route = route
                    .with_param(FieldContract::required("id", "Number"))
                    .with_query(FieldContract::required("limit", "Number"))
                    .with_header(FieldContract::required("x-flag", "Boolean"))
                    .with_header(FieldContract::required("x-count", "Number"))
                    .with_header(FieldContract::required("x-absent", "String"))
                    .with_response(200, None);
                route.options.request_validation = mode;
                route.options.response_validation = ValidationMode::None;
                if auth {
                    route.access = AccessLevel::Authenticated;
                }
                if mixed {
                    parsers.insert(&route, BodyParserHooks::default()).unwrap();
                }
                validators
                    .insert_request_transform(&route, &parsers, rejection(), move |request, raw| {
                        transform(request, raw, mixed)
                    })
                    .unwrap();
                router = if mixed {
                    router.post(route, report)
                } else {
                    router.get(route, report)
                };
            }
        }
    }
    let app = router
        .with_config(
            RouterConfig {
                request_validators: validators,
                json_request_parsers: parsers,
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
