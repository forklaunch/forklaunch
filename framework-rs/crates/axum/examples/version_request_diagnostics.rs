//! Trusted complete request schemas preserve public ordered diagnostics across parsers.
use axum::{Extension, Json, extract::RawQuery};
use forklaunch_axum::{
    AuthErrorFormat, CorrelationIdFormat, ForkLaunchRouter, ParserErrorFormat, RouterConfig,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, BodyKind, FieldContract, RequestContract, ResponseContract,
    RouteContract, SchemaRef, ValidationMode, VersionedContract,
};
use forklaunch_core::http::{
    BodyParserHooks, FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion,
    VersionBodyHooksRegistry, VersionParsedBody, VersionRequestParseFailure, VersionRequestParser,
    VersionRequestProjection, parse_typescript_simple_query,
};
use forklaunch_validator::typescript_coerce::{
    ZodJsonField, ZodJsonLiteral, ZodJsonOutcome, ZodJsonSchema, zod_json,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Default)]
struct Counts {
    handlers: AtomicUsize,
    parsers: AtomicUsize,
    verifiers: AtomicUsize,
}
fn contract(
    surface: &str,
    kind: &str,
    mode: ValidationMode,
    name: &str,
    auth: bool,
) -> RouteContract {
    let mut body = BodyContract::json("Input");
    body.kind = match kind {
        "text" => BodyKind::Text,
        "form" => BodyKind::UrlEncoded,
        _ => BodyKind::Json,
    };
    body.content_type = Some(
        match kind {
            "text" => "text/plain",
            "form" => "application/x-www-form-urlencoded",
            _ => "application/json",
        }
        .into(),
    );
    let mut route = RouteContract::post(
        format!("{surface}{kind}{name}{auth}"),
        "Checked request",
        format!("/{kind}/{name}/{auth}"),
    );
    route.options.request_validation = mode;
    route.options.response_validation = ValidationMode::None;
    if auth {
        route.access = AccessLevel::Protected;
    }
    for version in ["first", "last"] {
        let field = |name: &str| {
            let mut field = FieldContract::required(name, "Tag");
            field.schema =
                SchemaRef::named("Tag").with_json_schema(json!({"const":"ok"}).to_string());
            field
        };
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract {
                headers: vec![field("x-tag")],
                query: vec![field("tag")],
                ..RequestContract::default().with_body(body.clone())
            },
            responses: vec![ResponseContract::new(200, None)],
        });
    }
    route
}
fn configure(
    registry: &mut VersionBodyHooksRegistry,
    route: &RouteContract,
    kind: &str,
    counts: &Arc<Counts>,
) {
    registry
        .enable_selection_before_authentication(route)
        .unwrap();
    registry.enable_typescript_simple_query(route).unwrap();
    match kind {
        "text" => registry.enable_typescript_text_wire(route).unwrap(),
        "json" => registry.enable_typescript_json_wire(route).unwrap(),
        _ => {}
    }
    let counted = counts.clone();
    registry
        .insert_parser(
            route,
            BodyParserHooks::default().with_verify(move |_, _, _| {
                counted.verifiers.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    for version in ["first", "last"] {
        let counted = counts.clone();
        let text = kind == "text";
        registry
            .insert_request_parser(
                route,
                version,
                VersionRequestParser::new(move |input| {
                    counted.parsers.fetch_add(1, Ordering::SeqCst);
                    if input.body.is_some_and(|body| {
                        body == "operational" || body.get("flag") == Some(&json!("operational"))
                    }) {
                        return Err(VersionRequestParseFailure::Operational);
                    }
                    let default = json!("ok");
                    let literal = || ZodJsonSchema::Literal(ZodJsonLiteral::String("ok"));
                    let object =
                        |name, schema| ZodJsonSchema::Object(vec![ZodJsonField { name, schema }]);
                    let body = if text {
                        literal()
                    } else {
                        object(
                            "flag",
                            ZodJsonSchema::Default {
                                schema: Box::new(ZodJsonSchema::Optional(Box::new(literal()))),
                                value: &default,
                            },
                        )
                    };
                    let schema = ZodJsonSchema::Object(vec![
                        ZodJsonField {
                            name: "params",
                            schema: ZodJsonSchema::Unknown,
                        },
                        ZodJsonField {
                            name: "headers",
                            schema: object("x-tag", literal()),
                        },
                        ZodJsonField {
                            name: "query",
                            schema: object("tag", literal()),
                        },
                        ZodJsonField {
                            name: "body",
                            schema: body,
                        },
                    ]);
                    match zod_json(Some(&input.to_json()), &schema) {
                        Ok(ZodJsonOutcome::Valid(Some(value))) => {
                            VersionRequestProjection::from_json(value)
                        }
                        Ok(ZodJsonOutcome::Invalid(errors)) => {
                            Err(VersionRequestParseFailure::Validation(errors))
                        }
                        _ => Err(VersionRequestParseFailure::Operational),
                    }
                }),
            )
            .unwrap();
    }
}
fn output(
    selected: Option<&SelectedVersion>,
    parsed: Option<&VersionParsedBody>,
    query: Option<&str>,
) -> Value {
    json!({"selected":selected.map(|v|v.version.as_str()),"body":selected.and_then(|v|v.parsed_body.as_deref()).or_else(||parsed.and_then(VersionParsedBody::value)),"query":selected.map_or_else(||parse_typescript_simple_query(query), |v|v.parsed_fields.query.clone())})
}
#[tokio::main]
async fn main() {
    let counts = Arc::new(Counts::default());
    let mut hooks = VersionBodyHooksRegistry::default();
    let mut native = ForkLaunchRouter::anonymous();
    let mut local = LocalRouter::new();
    let mut contracts = vec![];
    for kind in ["json", "text", "form"] {
        for (name, mode) in [
            ("error", ValidationMode::Error),
            ("warning", ValidationMode::Warning),
            ("none", ValidationMode::None),
        ] {
            for auth in [false, true] {
                let route = contract("native", kind, mode, name, auth);
                configure(&mut hooks, &route, kind, &counts);
                let counted = counts.clone();
                native = native.route(
                    route,
                    move |selected: Option<Extension<SelectedVersion>>,
                          parsed: Option<Extension<VersionParsedBody>>,
                          RawQuery(query): RawQuery| {
                        counted.handlers.fetch_add(1, Ordering::SeqCst);
                        async move {
                            Json(output(
                                selected.as_ref().map(|v| &v.0),
                                parsed.as_ref().map(|v| &v.0),
                                query.as_deref(),
                            ))
                        }
                    },
                );
                let route = contract("local", kind, mode, name, auth);
                configure(&mut hooks, &route, kind, &counts);
                contracts.push(route.clone());
                let counted = counts.clone();
                local.register_with_body_hooks(route, &hooks, move |request: FrameworkRequest| {
                    counted.handlers.fetch_add(1, Ordering::SeqCst);
                    Ok(FrameworkResponse::json(
                        200,
                        output(
                            request.version.as_ref(),
                            request.version_parsed_body.as_ref(),
                            request.raw_query.as_deref(),
                        )
                        .to_string(),
                    ))
                });
            }
        }
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let app=ForkLaunchRouter::anonymous().nest("/native",native).nest("/local",local)
        .with_config(RouterConfig{version_body_hooks:hooks,parser_error_format:ParserErrorFormat::TypeScriptText,auth_error_format:AuthErrorFormat::TypeScriptText,correlation_id_format:CorrelationIdFormat::TypeScript,..RouterConfig::default()})
        .get(RouteContract::get("Counts","Counts","/calls").with_response(200,None),move||{let result=json!({"handlers":counts.handlers.load(Ordering::SeqCst),"parsers":counts.parsers.load(Ordering::SeqCst),"verifiers":counts.verifiers.load(Ordering::SeqCst)});async move{Json(result)}}).into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
