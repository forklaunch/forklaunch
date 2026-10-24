//! Version selection using opt-in default Express query parsing.
use axum::{Extension, Json};
use forklaunch_axum::{ForkLaunchRouter, RouterConfig};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::{
    BodyContract, FieldContract, RequestContract, ResponseContract, RouteContract, SchemaRef,
    VersionedContract,
};
use forklaunch_core::http::{
    FrameworkRequest, FrameworkResponse, LocalRouter, SelectedVersion, VersionBodyHooksRegistry,
};
use serde_json::{Value, json};
fn contract(surface: &str, overlap: bool) -> RouteContract {
    let mut route = RouteContract::get(
        format!("{surface}{overlap}"),
        "Query",
        format!("/query/{overlap}"),
    );
    let array = json!({"type":"array","items":{"type":"string"}});
    let mut schemas = vec![
        ("array", array.clone()),
        ("scalar", json!({"type":"string"})),
    ];
    if overlap {
        schemas.push(("last", json!({"anyOf":[array,{"type":"string"}]})));
    }
    for (name, schema) in schemas {
        let mut field = FieldContract::required("tag", "Tag");
        field.schema = SchemaRef::named("Tag").with_json_schema(schema.to_string());
        route = route.with_version(VersionedContract {
            version: name.into(),
            request: RequestContract {
                query: vec![field],
                ..RequestContract::default()
            },
            responses: vec![ResponseContract::new(
                200,
                Some(BodyContract::json("Output")),
            )],
        });
    }
    route
}
fn output(version: &SelectedVersion) -> Value {
    json!({"version":version.version,"query":version.parsed_fields.query})
}
#[tokio::main]
async fn main() {
    let mut hooks = VersionBodyHooksRegistry::default();
    let mut native = ForkLaunchRouter::new(ServiceInfo::new("native", "1"));
    let mut local = LocalRouter::new();
    let mut contracts = vec![];
    for overlap in [false, true] {
        let route = contract("native", overlap);
        hooks.enable_typescript_simple_query(&route).unwrap();
        native = native.get(
            route,
            |Extension(version): Extension<SelectedVersion>| async move { Json(output(&version)) },
        );
        let route = contract("local", overlap);
        hooks.enable_typescript_simple_query(&route).unwrap();
        contracts.push(route.clone());
        local.register_with_body_hooks(route, &hooks, |request: FrameworkRequest| {
            Ok(FrameworkResponse::json(
                200,
                output(request.version.as_ref().unwrap()).to_string(),
            ))
        });
    }
    let local =
        ForkLaunchRouter::from_local_router(ServiceInfo::new("local", "1"), contracts, local);
    let app = ForkLaunchRouter::anonymous()
        .nest("/native", native)
        .nest("/local", local)
        .with_config(RouterConfig {
            version_body_hooks: hooks,
            ..RouterConfig::default()
        })
        .into_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    println!("READY http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
