//! One registry for live HTTP/MCP and generated contract/SDK artifacts.
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, FieldContract, HmacAuth, RouteContract,
    SchemaRef,
};
use serde_json::{Value, json};
fn body(name: &str, schema: &Value) -> BodyContract {
    let mut body = BodyContract::json(name);
    body.schema.json_schema = Some(schema.to_string());
    body
}
fn uuid() -> Value {
    json!({"type":"string","format":"uuid"})
}
fn parameter(name: &str) -> FieldContract {
    let mut field = FieldContract::required(name, "String");
    field.schema.json_schema = Some(uuid().to_string());
    field
}
fn auth(route: RouteContract) -> RouteContract {
    let mut route = route
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ));
    for status in [400, 404, 409, 500, 502, 503] {
        route = route.with_response(
            status,
            Some(BodyContract {
                kind: BodyKind::Text,
                schema: SchemaRef::named("String"),
                content_type: Some("text/plain".into()),
            }),
        );
    }
    route
}
fn operation(state: &str, record: Option<Value>) -> Value {
    let mut schema = json!({"type":"object","additionalProperties":false,"properties":{"state":{"type":"string","enum":[state]},"operationId":uuid(),"publicId":uuid()},"required":["state","operationId"]});
    if state == "completed" || state == "created" {
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("publicId"));
    }
    if let Some(record) = record {
        schema["properties"]["record"] = record;
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("record"));
    }
    schema
}
/// Adds only the explicit `BoundCheckoutV1` routes; generic billing contracts stay unchanged.
/// # Panics
/// Only malformed package-owned Checkout response metadata can panic.
#[must_use]
pub fn bound_checkout_contracts() -> Vec<RouteContract> {
    let registry = forklaunch_billing_interface::module_contracts::module_registry(true);
    let read = registry
        .routes()
        .iter()
        .find(|r| r.operation_name == "getCheckoutSession")
        .unwrap();
    let response = read
        .responses
        .iter()
        .find(|r| r.status_code == 200)
        .unwrap()
        .body
        .clone()
        .unwrap();
    let mut record: Value =
        serde_json::from_str(response.schema.json_schema.as_ref().unwrap()).unwrap();
    record["properties"]["id"]["format"] = json!("uuid");
    record["properties"]["customerId"]["format"] = json!("uuid");
    let create = auth(RouteContract::post("Create Bound Checkout Session", "Create Bound Checkout Session", "/bound-checkout/operations/:operationId"))
        .with_param(parameter("operationId"))
        .with_request_body(body("BoundCheckoutCreateRequest",&json!({"type":"object","additionalProperties":false,"properties":{"localCustomerId":uuid(),"localPlanId":uuid(),"quantity":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_u64}},"required":["localCustomerId","localPlanId","quantity"]})))
        .with_response(200,Some(body("BoundCheckoutCreatedOrHistorical",&json!({"anyOf":[operation("created",Some(record.clone())),operation("completed",None)]}))))
        .with_response(202,Some(body("BoundCheckoutUnresolved",&json!({"anyOf":[operation("pending",None),operation("recordedEvidenceNoLongerCurrent",None)]}))));
    let get = auth(RouteContract::get(
        "Get Bound Created Checkout Session",
        "Get Bound Created Checkout Session",
        "/bound-checkout/sessions/:id",
    ))
    .with_param(parameter("id"))
    .with_response(200, Some(body("BoundCheckoutSession", &record)));
    let inspect = auth(RouteContract::get(
        "Inspect Bound Checkout Operation",
        "Inspect Bound Checkout Operation",
        "/bound-checkout/operations/:operationId",
    ))
    .with_param(parameter("operationId"))
    .with_response(
        200,
        Some(body(
            "BoundCheckoutOperation",
            &json!({"anyOf":[operation("pending",None),operation("completed",None)]}),
        )),
    );
    vec![create, get, inspect]
}
