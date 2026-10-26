//! Live framework fallback target for the actual TypeScript router oracle.
use axum::http::StatusCode;
use forklaunch_axum::{ForkLaunchRouter, ParserErrorFormat, RouterConfig, RouterOriginFormat};
use forklaunch_core::contract::RouteContract;

#[tokio::main]
async fn main() {
    let router = ForkLaunchRouter::anonymous()
        .get(
            RouteContract::get("List", "List", "/users").with_response(200, None),
            || async { StatusCode::OK },
        )
        .get(
            RouteContract::get("Read", "Read", "/users/:id").with_response(200, None),
            || async { StatusCode::OK },
        )
        .with_config(RouterConfig {
            router_origin_format: RouterOriginFormat::TypeScriptHtml,
            parser_error_format: ParserErrorFormat::TypeScriptText,
            ..RouterConfig::default()
        })
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, router).await.unwrap();
}
