//! Exercises the actual router's documentation responses for cross-language checks.
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use forklaunch_axum::{ForkLaunchRouter, RouterConfig};
use forklaunch_core::ServiceInfo;
use serde_json::{Value, json};
use std::io::{self, BufRead};
use tower::ServiceExt;

#[tokio::main]
async fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let router = ForkLaunchRouter::new(ServiceInfo::new("docs", "0.1.0"))
            .with_config(RouterConfig {
                docs_title: input["title"].as_str().unwrap().into(),
                openapi_path: input["url"].as_str().unwrap().into(),
                docs_sources: serde_json::from_value(input["sources"].clone()).unwrap(),
                swagger_enabled: true,
                ..RouterConfig::default()
            })
            .into_router();
        let mut responses = serde_json::Map::new();
        for path in ["/docs", "/swagger"] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status().as_u16();
            let content_type = response.headers()["content-type"]
                .to_str()
                .unwrap()
                .to_owned();
            let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            responses.insert(path.into(), json!({"status":status,"contentType":content_type,"body":String::from_utf8(body.to_vec()).unwrap()}));
        }
        println!(
            "{}",
            Value::Object(responses)
                .to_string()
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029")
        );
    }
}
