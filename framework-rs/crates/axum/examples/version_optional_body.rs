//! Explicit optional parsed-body contracts; supplied values retain their schemas.
use axum::{Extension, Json};
use forklaunch_axum::{CorrelationIdFormat, ForkLaunchRouter, ParserErrorFormat, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    BodyContract, BodyKind, HttpMethod, RequestContract, ResponseContract, RouteContract,
    ValidationMode, VersionedContract,
};
use forklaunch_core::http::{
    FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion, VersionBodyHooks,
    VersionBodyHooksRegistry, VersionParsedBody,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn contract(
    surface: &str,
    kind: &str,
    mode_name: &str,
    mode: ValidationMode,
    form: &str,
    method: HttpMethod,
) -> RouteContract {
    let mut body = BodyContract::json("Input");
    if kind == "text" {
        body.kind = BodyKind::Text;
        body.content_type = Some("text/plain".into());
    }
    body.schema = body.schema.with_json_schema(
        if kind == "json" {
            json!({"type":"object","required":["flag"],"properties":{"flag":{"type":"string"}}})
        } else {
            json!({"type":"string","const":"ok"})
        }
        .to_string(),
    );
    let mut route = RouteContract::post(
        format!("{surface}{kind}{mode_name}{form}{}", method.as_str()),
        "Optional body",
        format!("/{kind}/{mode_name}/{form}"),
    );
    route.method = method;
    route.options.request_validation = mode;
    route.options.response_validation = ValidationMode::None;
    for version in ["first", "last"] {
        let request = if form == "required" {
            RequestContract::default().with_body(body.clone())
        } else {
            RequestContract::default().with_optional_body(body.clone())
        };
        route = route.with_version(VersionedContract {
            version: version.into(),
            request,
            responses: vec![ResponseContract::new(200, None)],
        });
    }
    route
}
fn output(selected: Option<&SelectedVersion>, parsed: Option<&VersionParsedBody>) -> Value {
    let body = selected.map_or_else(
        || parsed.and_then(VersionParsedBody::value),
        |selected| selected.parsed_body.as_deref(),
    );
    json!({"selected":selected.map(|v|v.version.as_str()),"present":body.is_some(),"body":body})
}
fn configure(
    hooks: &mut VersionBodyHooksRegistry,
    route: &RouteContract,
    kind: &str,
    form: &str,
    count: &Arc<AtomicUsize>,
) {
    if kind == "json" {
        hooks.enable_typescript_json_wire(route).unwrap();
    } else {
        hooks.enable_typescript_text_wire(route).unwrap();
    }
    hooks.enable_selection_before_authentication(route).unwrap();
    for version in ["first", "last"] {
        let default = if kind == "json" {
            json!({"flag":"default"})
        } else {
            json!("ok")
        };
        let transformed = if kind == "json" {
            json!({"flag":"transformed"})
        } else {
            json!("transformed")
        };
        let count = count.clone();
        let hook = match form {
            "default" => VersionBodyHooks::default().with_optional_preprocess(move |value| {
                Ok(Some(value.cloned().unwrap_or_else(|| default.clone())))
            }),
            "transform" => VersionBodyHooks::default().with_optional_transform(move |value| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(Some(value.cloned().unwrap_or_else(|| transformed.clone())))
            }),
            _ => VersionBodyHooks::default(),
        };
        hooks.insert(route, version, hook).unwrap();
    }
}
#[tokio::main]
async fn main() {
    let handlers = Arc::new(AtomicUsize::new(0));
    let transforms = Arc::new(AtomicUsize::new(0));
    let mut hooks = VersionBodyHooksRegistry::default();
    let mut native = ForkLaunchRouter::new(ServiceInfo::new("native", "1"));
    let mut local = LocalRouter::new();
    let mut contracts = Vec::new();
    for kind in ["json", "text"] {
        for (name, mode) in [
            ("none", ValidationMode::None),
            ("warning", ValidationMode::Warning),
            ("error", ValidationMode::Error),
        ] {
            for form in ["required", "optional", "default", "transform"] {
                for method in [HttpMethod::Post, HttpMethod::Delete] {
                    let route = contract("native", kind, name, mode, form, method);
                    configure(&mut hooks, &route, kind, form, &transforms);
                    let count = handlers.clone();
                    native=native.route(route,move |selected:Option<Extension<SelectedVersion>>,parsed:Option<Extension<VersionParsedBody>>|{count.fetch_add(1,Ordering::SeqCst);async move{Json(output(selected.as_ref().map(|v|&v.0),parsed.as_ref().map(|v|&v.0)))}});
                    let route = contract("local", kind, name, mode, form, method);
                    configure(&mut hooks, &route, kind, form, &transforms);
                    contracts.push(route.clone());
                    let count = handlers.clone();
                    local.register_with_body_hooks(
                        route,
                        &hooks,
                        move |request: FrameworkRequest| {
                            count.fetch_add(1, Ordering::SeqCst);
                            Ok(FrameworkResponse::json(
                                200,
                                output(
                                    request.version.as_ref(),
                                    request.version_parsed_body.as_ref(),
                                )
                                .to_string(),
                            ))
                        },
                    );
                }
            }
        }
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let router=ForkLaunchRouter::new(ServiceInfo::new("optional","1")).nest("/native",native).nest("/local",local).get(RouteContract::get("Calls","Calls","/calls").with_response(200,None),move ||{let value=json!({"handlers":handlers.load(Ordering::SeqCst),"transforms":transforms.load(Ordering::SeqCst)});async move{Json(value)}}).with_config(RouterConfig{version_body_hooks:hooks,parser_error_format:ParserErrorFormat::TypeScriptText,correlation_id_format:CorrelationIdFormat::TypeScript,..RouterConfig::default()});
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    #[cfg(feature = "mcp")]
    let app = router
        .into_router_with_mcp(forklaunch_core::mcp::server::McpServerOptions::new(
            "optional", "1",
        ))
        .unwrap();
    #[cfg(not(feature = "mcp"))]
    let app = router.into_router();
    axum::serve(listener, app).await.unwrap();
}
