//! Finite unversioned JSON parser target for the actual-source HTTP oracle.
use axum::{Extension, Json};
use forklaunch_axum::{
    CorrelationIdFormat, ForkLaunchRouter, JsonRequestParsers, ParsedJsonBody, ParserErrorFormat,
    RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{AccessLevel, BodyContract, RouteContract, ValidationMode};
use forklaunch_core::http::BodyParserHooks;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn numbers(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Number(value) => output.push(format!("{:016x}", value.as_f64().unwrap().to_bits())),
        Value::Array(values) => values.iter().for_each(|value| numbers(value, output)),
        Value::Object(values) => {
            let mut values: Vec<_> = values.iter().collect();
            values.sort_by_key(|(key, _)| *key);
            for (_, value) in values {
                numbers(value, output);
            }
        }
        _ => {}
    }
}
fn shape(value: &Value) -> Value {
    match value {
        Value::Number(_) => json!("<number>"),
        Value::Array(values) => Value::Array(values.iter().map(shape).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), shape(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}
fn result(parsed: ParsedJsonBody) -> Value {
    let mut output = json!({"kind":"undefined", "numbers":[]});
    if let Some(value) = parsed.value {
        output["kind"] = json!(match value.as_ref() {
            Value::Null => "null",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::String(_) => "string",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
        });
        let mut bits = Vec::new();
        numbers(&value, &mut bits);
        output["numbers"] = json!(bits);
        output["value"] = shape(&value);
    }
    if let Some(bytes) = parsed.raw_body {
        output["raw"] = json!(bytes.iter().fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").unwrap();
            output
        }));
    }
    output
}
#[tokio::main]
async fn main() {
    let reviver = std::env::args().nth(2).as_deref() == Some("true");
    let revived_numbers = Arc::new(Mutex::new(Vec::<String>::new()));
    let strict = std::env::args().nth(1).as_deref() != Some("false");
    let handlers = Arc::new(AtomicUsize::new(0));
    let verifiers = Arc::new(AtomicUsize::new(0));
    let mut parsers = JsonRequestParsers::default();
    let mut router = ForkLaunchRouter::new(ServiceInfo::new("json-parser", "1"));
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
    ] {
        for auth in [false, true] {
            let mut route = RouteContract::post(
                format!("Parse{name}{auth}"),
                "Parse",
                format!("/parse/{name}/{auth}"),
            )
            .with_request_body(BodyContract::json("Input"))
            .with_response(200, None);
            route.options.request_validation = mode;
            route.options.response_validation = ValidationMode::None;
            route.options.body_parser.json_wire.limit_bytes = 128;
            route.options.body_parser.json_strict = strict;
            if auth {
                route.access = AccessLevel::Authenticated;
            }
            let verify = verifiers.clone();
            let trace = revived_numbers.clone();
            let mut hooks = BodyParserHooks::default().with_verify(move |request, _, _| {
                trace.lock().unwrap().clear();
                verify.fetch_add(1, Ordering::SeqCst);
                if request
                    .headers
                    .get("x-deny")
                    .is_some_and(|value| value == "yes")
                {
                    Err("PRIVATE_VERIFY".into())
                } else {
                    Ok(())
                }
            });
            if reviver {
                let trace = revived_numbers.clone();
                hooks = hooks.with_json_reviver(move |_, value| {
                    if let Value::Number(number) = &value {
                        trace
                            .lock()
                            .unwrap()
                            .push(format!("{:016x}", number.as_f64().unwrap().to_bits()));
                    }
                    Ok(Some(value))
                });
            }
            parsers.insert(&route, hooks).unwrap();
            let calls = handlers.clone();
            let trace = revived_numbers.clone();
            router = router.post(
                route,
                move |Extension(parsed): Extension<ParsedJsonBody>| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let mut output = result(parsed);
                    if reviver {
                        output["revivedNumbers"] = json!(trace.lock().unwrap().clone());
                    }
                    Json(output)
                },
            );
        }
    }
    let app = router.get(RouteContract::get("Calls", "Calls", "/calls").with_response(200, None), move || async move {
        Json(json!({"handlerCalls":handlers.load(Ordering::SeqCst),"verifyCalls":verifiers.load(Ordering::SeqCst)}))
    }).with_config(RouterConfig { json_request_parsers:parsers, parser_error_format:ParserErrorFormat::TypeScriptText,
        correlation_id_format:CorrelationIdFormat::TypeScript, ..RouterConfig::default() }).into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
