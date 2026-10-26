//! Raw HTTP correlation target for the actual TypeScript source oracle.
use axum::{Extension, http::StatusCode};
use forklaunch_axum::{
    CorrelationIdFormat, ForkLaunchRouter, HandlerErrorFormat, ParserErrorFormat, RequestContext,
    RouterConfig, RouterOriginFormat,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    AccessLevel, BodyContract, RequestContract, ResponseContract, RouteContract, VersionedContract,
};
use forklaunch_core::http::{FrameworkRequest, FrameworkResponse, LocalRouter};

#[tokio::main]
async fn main() {
    let failure = RouteContract::get("Throw", "Throw", "/throw").with_response(200, None);
    let mut handlers = LocalRouter::new();
    handlers.register(
        failure.clone(),
        |_: FrameworkRequest| -> Result<FrameworkResponse, forklaunch_common::FrameworkError> {
            Err(forklaunch_common::FrameworkError::new("private", "PRIVATE"))
        },
    );
    let router = ForkLaunchRouter::from_local_router(
        ServiceInfo::new("correlation", "1"),
        [failure],
        handlers,
    )
    .get(
        RouteContract::get("Context", "Context", "/context").with_response(200, None),
        |Extension(context): Extension<RequestContext>| async move { context.correlation_id },
    )
    .get(
        RouteContract::get("Override", "Override", "/override").with_response(200, None),
        |Extension(context): Extension<RequestContext>| async move {
            (
                [("x-correlation-id", "application")],
                context.correlation_id,
            )
        },
    )
    .get(
        RouteContract::get("Auth", "Auth", "/auth")
            .with_access(AccessLevel::Authenticated)
            .with_response(200, None),
        || async { StatusCode::OK },
    )
    .get(
        RouteContract::get("Param", "Param", "/param/:id").with_response(200, None),
        || async { StatusCode::OK },
    )
    .post(
        RouteContract::post("Parse", "Parse", "/parse").with_version(VersionedContract {
            version: "v1".into(),
            request: RequestContract::default().with_body(BodyContract::json("Input")),
            responses: vec![ResponseContract::new(200, None)],
        }),
        || async { StatusCode::OK },
    )
    .with_config(RouterConfig {
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
