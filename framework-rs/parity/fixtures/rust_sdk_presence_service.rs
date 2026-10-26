use axum::{Json, extract::Path};
use forklaunch_axum::ForkLaunchRouter;
use forklaunch_core::contract::{BodyContract, FieldContract, RouteContract, SchemaRef};
use serde_json::{Value, json};

fn body() -> BodyContract {
    BodyContract {
        schema: SchemaRef::named("Presence").with_json_schema(include_str!("presence_schema.json")),
        ..BodyContract::json("Presence")
    }
}
pub fn contracts() -> Vec<RouteContract> {
    vec![
        RouteContract::get("Read Presence", "Typed field presence", "/presence/:index")
            .with_param(FieldContract::required("index", "String"))
            .with_response(200, Some(body())),
        RouteContract::post("Write Presence", "Presence-preserving echo", "/presence")
            .with_request_body(body())
            .with_response(200, Some(body())),
    ]
}
pub fn register(router: ForkLaunchRouter) -> ForkLaunchRouter {
    let mut routes = contracts().into_iter();
    router
        .get(
            routes.next().unwrap(),
            |Path(index): Path<usize>| async move {
                let cases: Value =
                    serde_json::from_str(include_str!("presence_cases.json")).unwrap();
                Json(
                    cases
                        .get(index)
                        .map_or_else(|| json!({}), |case| case["value"].clone()),
                )
            },
        )
        .post(
            routes.next().unwrap(),
            |Json(value): Json<Value>| async move { Json(value) },
        )
}
