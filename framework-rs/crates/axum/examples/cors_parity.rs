//! CORS HTTP target for actual TypeScript source comparisons.
use axum::http::{HeaderValue, StatusCode};
use forklaunch_axum::{
    CorrelationIdFormat, ForkLaunchRouter, HandlerErrorFormat, ParserErrorFormat, RouterConfig,
    RouterOriginFormat, TypeScriptCors, TypeScriptCorsOrigin, TypeScriptCorsOriginDecision,
    TypeScriptCorsOriginDelegate, TypeScriptCorsOriginError,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{HttpMethod, RouteContract};
use forklaunch_core::http::{FrameworkRequest, FrameworkResponse, LocalRouter};
use serde_json::Value;

fn header(value: &Value) -> HeaderValue {
    let value = value.as_array().map_or_else(
        || value.as_str().unwrap().to_owned(),
        |values| {
            values
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",")
        },
    );
    HeaderValue::from_str(&value).unwrap()
}

fn origin_policy(input: &Value) -> TypeScriptCorsOrigin {
    let mut origin = match input.get("origin") {
        None => TypeScriptCorsOrigin::Any,
        Some(Value::Bool(false)) => TypeScriptCorsOrigin::Disabled,
        Some(Value::Bool(true)) => TypeScriptCorsOrigin::Reflect,
        Some(Value::Array(values)) => {
            TypeScriptCorsOrigin::AllowList(values.iter().map(header).collect())
        }
        Some(value) => TypeScriptCorsOrigin::Fixed(header(value)),
    };
    if let Some(mode) = input["delegate"].as_str() {
        let mode = mode.to_owned();
        origin =
            TypeScriptCorsOrigin::Delegate(TypeScriptCorsOriginDelegate::new(move |_origin| {
                let mode = mode.clone();
                async move {
                    if mode.starts_with("async") {
                        tokio::task::yield_now().await;
                    }
                    if mode.contains("error") {
                        return Err(TypeScriptCorsOriginError);
                    }
                    Ok(match mode.as_str() {
                        "true" | "async-true" => TypeScriptCorsOriginDecision::Reflect,
                        "fixed" => TypeScriptCorsOriginDecision::Fixed(HeaderValue::from_static(
                            "https://fixed.example",
                        )),
                        "list" => {
                            TypeScriptCorsOriginDecision::AllowList(vec![HeaderValue::from_static(
                                "https://allowed.example",
                            )])
                        }
                        _ => TypeScriptCorsOriginDecision::Disabled,
                    })
                }
            }));
    }
    origin
}

#[tokio::main]
async fn main() {
    let input: Value = serde_json::from_str(&std::env::args().nth(1).unwrap()).unwrap();
    let origin = origin_policy(&input);
    let cors = TypeScriptCors {
        origin,
        methods: input
            .get("methods")
            .map_or_else(|| TypeScriptCors::default().methods, header),
        allowed_headers: input.get("allowedHeaders").map(header),
        exposed_headers: input.get("exposedHeaders").map(header),
        credentials: input["credentials"].as_bool().unwrap_or(false),
        max_age: input["maxAge"].as_u64(),
        max_age_number: input.get("maxAgeNumber").map(|value| {
            value
                .as_f64()
                .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
        }),
        preflight_continue: input["preflightContinue"].as_bool().unwrap_or(false),
        options_success_status: StatusCode::from_u16(
            input["optionsSuccessStatus"]
                .as_u64()
                .map_or(204, |value| u16::try_from(value).unwrap()),
        )
        .unwrap(),
    };
    let failure = RouteContract::get("Error", "Error", "/error").with_response(200, None);
    let mut handlers = LocalRouter::new();
    handlers.register(
        failure.clone(),
        |_: FrameworkRequest| -> Result<FrameworkResponse, forklaunch_common::FrameworkError> {
            Err(forklaunch_common::FrameworkError::new("private", "PRIVATE"))
        },
    );
    let router =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("cors", "1"), [failure], handlers)
            .get(
                RouteContract::get("Param", "Param", "/param/:id").with_response(200, None),
                || async { "ok" },
            )
            .get(
                RouteContract::get("Ok", "Ok", "/ok").with_response(200, None),
                || async { ([("x-handler", "get")], "ok") },
            )
            .get(
                RouteContract::get("Override", "Override", "/override").with_response(200, None),
                || async {
                    (
                        [
                            ("access-control-allow-origin", "application"),
                            ("vary", "Accept-Encoding"),
                        ],
                        "override",
                    )
                },
            )
            .route(
                RouteContract::new("Options", "Options", HttpMethod::Options, "/explicit")
                    .with_response(202, None),
                || async { (StatusCode::ACCEPTED, [("x-handler", "options")], "explicit") },
            )
            .with_config(RouterConfig {
                typescript_cors: Some(cors),
                correlation_id_format: CorrelationIdFormat::TypeScript,
                router_origin_format: RouterOriginFormat::TypeScriptHtml,
                parser_error_format: ParserErrorFormat::TypeScriptText,
                handler_error_format: HandlerErrorFormat::TypeScriptText,
                ..RouterConfig::default()
            })
            .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, router).await.unwrap();
}
