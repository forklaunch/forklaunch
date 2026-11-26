use forklaunch_axum::native::{ContractRouter, VersionedHandlers};
use forklaunch_core::contract::{
    ContractRegistry, HttpMethod,
    endpoint::{Endpoint, Fields, JsonBody, NoBody, VersionedEndpoint},
};
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct OldQuery {
    #[schemars(regex(pattern = "^(both|first)$"))]
    select: String,
    body: String,
    header: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
struct NewQuery {
    #[schemars(regex(pattern = "^(both|last)$"))]
    select: String,
    body: String,
    header: String,
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct OldOutput {
    #[schemars(regex(pattern = "^old$"))]
    value: String,
}
#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct NewOutput {
    #[schemars(regex(pattern = "^new$"))]
    value: String,
}
#[derive(serde::Serialize, schemars::JsonSchema)]
struct OldHeader {
    #[serde(rename = "x-branch")]
    #[schemars(regex(pattern = "^old$"))]
    value: String,
}
#[derive(serde::Serialize, schemars::JsonSchema)]
struct NewHeader {
    #[serde(rename = "x-branch")]
    #[schemars(regex(pattern = "^new$"))]
    value: String,
}

#[tokio::main]
async fn main() {
    use forklaunch_core::contract::endpoint::ResponseWithHeaders;
    type OldInput = Fields<NoBody, OldQuery>;
    type NewInput = Fields<NoBody, NewQuery>;
    let old = Endpoint::<OldInput, JsonBody<OldOutput>>::new(
        "Union",
        "Union",
        HttpMethod::Get,
        "/union",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<OldHeader>()
    .unwrap();
    let new = Endpoint::<NewInput, JsonBody<NewOutput>>::new(
        "Union",
        "Union",
        HttpMethod::Get,
        "/union",
        200,
    )
    .unwrap()
    .with_typed_response_headers::<NewHeader>()
    .unwrap();
    let descriptor = VersionedEndpoint::new("first", &old)
        .unwrap()
        .with_version("last", &new)
        .unwrap();
    let handlers = VersionedHandlers::new(descriptor.clone())
        .bind("first", old, |_, (), value| {
            assert!(matches!(value.query.select.as_str(), "both" | "first"));
            std::future::ready(Ok(ResponseWithHeaders {
                body: OldOutput {
                    value: value.query.body,
                },
                headers: OldHeader {
                    value: value.query.header,
                },
            }))
        })
        .unwrap()
        .bind("last", new, |_, (), value| {
            assert!(matches!(value.query.select.as_str(), "both" | "last"));
            std::future::ready(Ok(ResponseWithHeaders {
                body: NewOutput {
                    value: value.query.body,
                },
                headers: NewHeader {
                    value: value.query.header,
                },
            }))
        })
        .unwrap();
    let app = ContractRouter::<()>::new({
        let mut r = ContractRegistry::new();
        r.register(descriptor.contract());
        r
    })
    .bind_versioned(handlers)
    .unwrap()
    .finish()
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
