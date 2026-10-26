//! Finite actual-source registered-tool admission oracle.
#[cfg(feature = "rmcp-server")]
#[tokio::main]
async fn main() {
    use axum::{Json, Router, body::to_bytes, routing::any};
    use forklaunch_core::contract::{
        BodyContract, BodyKind, ContractRegistry, HttpMethod, RouteContract, SchemaRef,
    };
    use forklaunch_core::mcp::server::{
        McpBodyAdmission, McpServerOptions, MissingToolBody, mount_http,
    };
    use serde_json::json;
    let mut registry = ContractRegistry::default();
    let mut options = McpServerOptions::new("admission", "1");
    for kind in ["json", "text"] {
        for form in ["required", "optional", "nullable", "nullish", "default"] {
            for method in [HttpMethod::Post, HttpMethod::Delete] {
                let name = format!("{kind}_{form}_{}", method.as_str().to_lowercase());
                let mut route = RouteContract::post(&name, &name, "/wire");
                route.method = method;
                route.operation_name = name;
                let base = if kind == "json" {
                    json!({"type":"object","required":["flag"],"properties":{"flag":{"type":"string"}},"additionalProperties":false})
                } else {
                    json!({"type":"string"})
                };
                let schema = if matches!(form, "nullable" | "nullish") {
                    json!({"anyOf":[base,{"type":"null"}]})
                } else {
                    base
                };
                route.request.body = Some(BodyContract {
                    kind: if kind == "json" {
                        BodyKind::Json
                    } else {
                        BodyKind::Text
                    },
                    schema: SchemaRef::named("Body").with_json_schema(schema.to_string()),
                    content_type: Some(
                        if kind == "json" {
                            "application/json"
                        } else {
                            "text/plain"
                        }
                        .into(),
                    ),
                });
                let missing = if form == "default" {
                    MissingToolBody::Default(if kind == "json" {
                        json!({"flag":"default"})
                    } else {
                        json!("default")
                    })
                } else if matches!(form, "optional" | "nullish") {
                    MissingToolBody::Omit
                } else {
                    MissingToolBody::Reject
                };
                options
                    .body_admissions
                    .insert(
                        &route,
                        McpBodyAdmission::new(
                            missing,
                            matches!(form, "nullable" | "nullish"),
                            1024,
                            8,
                        ),
                    )
                    .unwrap();
                registry.register(route);
            }
        }
    }
    let router=Router::new().route("/wire",any(|request:axum::extract::Request|async move{
        let media=request.headers().get("content-type").and_then(|v|v.to_str().ok()).map(str::to_owned);
        let length=request.headers().get("content-length").and_then(|v|v.to_str().ok()).map(str::to_owned);
        let body=to_bytes(request.into_body(),2048).await.unwrap();
        Json(json!({"body":String::from_utf8(body.to_vec()).unwrap(),"length":length,"media":media}))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, mount_http(router, &registry, options).unwrap())
        .await
        .unwrap();
}
#[cfg(not(feature = "rmcp-server"))]
fn main() {}
