//! Caller-owned JSON schema transform wired through the reusable runtime API.
use axum::{Extension, Json};
use forklaunch_axum::{
    AuthErrorFormat, CorrelationIdFormat, ForkLaunchRouter, JsonRequestParsers, ParsedJsonBody,
    RequestValidators, RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, BodyKind, ResponseContract, RouteContract, SchemaRef, ValidationMode,
};
use forklaunch_core::http::typescript_request_validation_error_response;
use forklaunch_core::http::{
    BodyParserHooks, ErrorKind, ErrorResponse, FrameworkRequest, FrameworkResponse,
};
use forklaunch_validator::{ParseError, typescript_coerce};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn kind(value: Option<&Value>) -> &'static str {
    match value {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
        Some(Value::String(_)) => "string",
        Some(Value::Number(_)) => "number",
        Some(Value::Bool(_)) => "boolean",
    }
}
fn transform(request: &FrameworkRequest) -> Result<Option<Value>, FrameworkResponse> {
    let value = request
        .parsed_json
        .as_ref()
        .and_then(|parsed| parsed.value.as_deref());
    let mut errors = Vec::new();
    let Some(Value::Object(object)) = value else {
        errors.push(ParseError {
            path: vec!["body".into()],
            message: if value.is_none() {
                "Required".into()
            } else {
                format!("Expected object, received {}", kind(value))
            },
        });
        return Err(typescript_request_validation_error_response(
            Some(&errors),
            Some(&request.context.correlation_id),
        ));
    };
    let amount = typescript_coerce::number(object.get("amount")).map_err(|mut error| {
        error.path = vec!["body".into(), "amount".into()];
        errors.push(error);
    });
    let label = match object.get("label") {
        None => Some("fallback"),
        Some(Value::String(label)) => Some(label.as_str()),
        other => {
            errors.push(ParseError {
                path: vec!["body".into(), "label".into()],
                message: format!("Expected string, received {}", kind(other)),
            });
            None
        }
    };
    if !errors.is_empty() {
        return Err(typescript_request_validation_error_response(
            Some(&errors),
            Some(&request.context.correlation_id),
        ));
    }
    Ok(Some(
        json!({"amount":amount.unwrap(),"label":label.unwrap()}),
    ))
}
fn report(parsed: ParsedJsonBody) -> Value {
    let mut result = json!({"hasBody":parsed.value.is_some()});
    if let Some(value) = parsed.value {
        result["body"] = value.as_ref().clone();
    }
    if let Some(raw) = parsed.raw_body {
        result["raw"] = json!(String::from_utf8_lossy(&raw));
    }
    result
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
    let calls = Arc::new(AtomicUsize::new(0));
    let mut parsers = JsonRequestParsers::default();
    let mut validators = RequestValidators::default();
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("transform", "1"));
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        for profile in ["public", "protected", "default", "remove"] {
            let mut route = RouteContract::post(
                format!("Transform{name}{profile}"),
                "Transform",
                format!("/transform/{name}/{profile}"),
            )
            .with_request_body(BodyContract::json("Input"))
            .with_response(200, None);
            route.options.request_validation = mode;
            route.options.response_validation = ValidationMode::None;
            route.options.body_parser.json_strict = false;
            if profile == "protected" {
                route.access = AccessLevel::Authenticated;
            }
            parsers.insert(&route, BodyParserHooks::default()).unwrap();
            validators
                .insert_json_transform(
                    &route,
                    &parsers,
                    rejection(),
                    move |request| match profile {
                        "remove" => Ok(None),
                        "default" if request.parsed_json.as_ref().unwrap().value.is_none() => {
                            Ok(Some(json!({"amount":7,"label":"fallback"})))
                        }
                        _ => transform(request),
                    },
                )
                .unwrap();
            let count = calls.clone();
            router = router.post(
                route,
                move |Extension(parsed): Extension<ParsedJsonBody>| async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(report(parsed))
                },
            );
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
