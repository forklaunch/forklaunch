//! Opt-in first-version Text wire parsing; schema defaults remain caller-owned.
use axum::{Extension, Json};
use forklaunch_axum::{CorrelationIdFormat, ForkLaunchRouter, ParserErrorFormat, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    BodyContract, BodyKind, HttpMethod, RequestContract, ResponseContract, RouteContract,
    ValidationMode, VersionedContract,
};
use forklaunch_core::http::{
    BodyParserHooks, FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion,
    VersionBodyHooksRegistry, VersionParsedBody, VersionRequestParseFailure, VersionRequestParser,
    VersionRequestProjection,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn contract(
    surface: &str,
    name: &str,
    mode: ValidationMode,
    optional: bool,
    method: HttpMethod,
) -> RouteContract {
    let mut body = BodyContract::json("Input");
    body.kind = BodyKind::Text;
    body.content_type = Some("text/plain".into());
    body.schema = body
        .schema
        .with_json_schema(json!({"type":"string","const":"ok"}).to_string());
    let mut route = RouteContract::post(
        format!("{surface}{name}{optional}{}", method.as_str()),
        "Text wire",
        format!("/{name}/{optional}"),
    );
    route.method = method;
    route.options.request_validation = mode;
    route.options.response_validation = ValidationMode::None;
    route.options.body_parser.text_wire.limit_bytes = 128;
    for version in ["first", "last"] {
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract::default().with_body(body.clone()),
            responses: vec![ResponseContract::new(200, None)],
        });
    }
    route
}
fn output(selected: Option<&SelectedVersion>, parsed: Option<&VersionParsedBody>) -> Value {
    json!({"selected":selected.map(|selected|selected.version.as_str()),"body":selected.map_or_else(|| parsed.and_then(VersionParsedBody::value), |selected|selected.parsed_body.as_deref())})
}
fn configure(
    hooks: &mut VersionBodyHooksRegistry,
    route: &RouteContract,
    optional: bool,
    calls: Arc<AtomicUsize>,
) {
    hooks.enable_typescript_text_wire(route).unwrap();
    hooks.enable_selection_before_authentication(route).unwrap();
    hooks
        .insert_parser(
            route,
            BodyParserHooks::default().with_verify(move |request, _, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                if request
                    .headers
                    .get("x-deny")
                    .is_some_and(|value| value == "yes")
                {
                    Err("PRIVATE_VERIFY".into())
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    for version in ["first", "last"] {
        hooks
            .insert_request_parser(
                route,
                version,
                VersionRequestParser::new(move |input| {
                    use forklaunch_validator::typescript_coerce::{
                        ZodJsonLiteral, ZodJsonOutcome, ZodJsonSchema, zod_json_at_path,
                    };
                    let default = json!("ok");
                    let literal = ZodJsonSchema::Literal(ZodJsonLiteral::String("ok"));
                    let schema = if optional {
                        ZodJsonSchema::Default {
                            schema: Box::new(ZodJsonSchema::Optional(Box::new(literal))),
                            value: &default,
                        }
                    } else {
                        literal
                    };
                    match zod_json_at_path(input.body, &schema, &["body"]) {
                        Ok(ZodJsonOutcome::Valid(body)) => Ok(VersionRequestProjection {
                            fields: input.fields.clone(),
                            body,
                        }),
                        Ok(ZodJsonOutcome::Invalid(errors)) => {
                            Err(VersionRequestParseFailure::Validation(errors))
                        }
                        Err(_) => Err(VersionRequestParseFailure::Operational),
                    }
                }),
            )
            .unwrap();
    }
}
#[tokio::main]
async fn main() {
    let handlers = Arc::new(AtomicUsize::new(0));
    let verifiers = Arc::new(AtomicUsize::new(0));
    let mut hooks = VersionBodyHooksRegistry::default();
    let mut native = ForkLaunchRouter::new(ServiceInfo::new("native", "1"));
    let mut local = LocalRouter::new();
    let mut contracts = Vec::new();
    for (name, mode) in [
        ("none", ValidationMode::None),
        ("warning", ValidationMode::Warning),
        ("error", ValidationMode::Error),
    ] {
        for method in [HttpMethod::Post, HttpMethod::Delete] {
            for optional in [false, true] {
                let route = contract("native", name, mode, optional, method);
                configure(&mut hooks, &route, optional, verifiers.clone());
                let count = handlers.clone();
                native = native.route(
                    route,
                    move |selected: Option<Extension<SelectedVersion>>,
                          parsed: Option<Extension<VersionParsedBody>>| {
                        count.fetch_add(1, Ordering::SeqCst);
                        async move {
                            Json(output(
                                selected.as_ref().map(|selected| &selected.0),
                                parsed.as_ref().map(|parsed| &parsed.0),
                            ))
                        }
                    },
                );
                let route = contract("local", name, mode, optional, method);
                configure(&mut hooks, &route, optional, verifiers.clone());
                contracts.push(route.clone());
                let count = handlers.clone();
                local.register_with_body_hooks(route, &hooks, move |request: FrameworkRequest| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::json(
                        200,
                        output(
                            request.version.as_ref(),
                            request.version_parsed_body.as_ref(),
                        )
                        .to_string(),
                    ))
                });
            }
        }
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let router=ForkLaunchRouter::new(ServiceInfo::new("text-wire","1")).nest("/native",native).nest("/local",local)
        .get(RouteContract::get("Calls","Calls","/calls").with_response(200,None),move || {let value=json!({"handlers":handlers.load(Ordering::SeqCst),"verifiers":verifiers.load(Ordering::SeqCst)});async move {Json(value)}})
        .with_config(RouterConfig {version_body_hooks:hooks,parser_error_format:ParserErrorFormat::TypeScriptText,correlation_id_format:CorrelationIdFormat::TypeScript,..RouterConfig::default()});
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    #[cfg(feature = "mcp")]
    let app = router
        .into_router_with_mcp(forklaunch_core::mcp::server::McpServerOptions::new(
            "text-wire",
            "1",
        ))
        .unwrap();
    #[cfg(not(feature = "mcp"))]
    let app = router.into_router();
    axum::serve(listener, app).await.unwrap();
}
