//! Shared creation/inspection metadata; no private profile or historical provider DTO.
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, FieldContract, HmacAuth, RouteContract,
    SchemaRef,
};
use serde_json::{Value, json};
fn body(name: &str, schema: &Value) -> BodyContract {
    let mut result = BodyContract::json(name);
    result.schema.json_schema = Some(schema.to_string());
    result
}
fn uuid() -> Value {
    json!({"type":"string","format":"uuid"})
}
fn operation(state: &str) -> Value {
    let mut value = json!({"type":"object","additionalProperties":false,"properties":{"state":{"type":"string","enum":[state]},"operationId":uuid()},"required":["state","operationId"]});
    if state == "completed" {
        value["properties"]["publicId"] = uuid();
        value["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("publicId"));
    }
    value
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
/// Explicit trusted creation profile only; existing generic billing routes unchanged.
/// # Panics
/// Only malformed package-owned static metadata can panic.
#[must_use]
pub fn bound_plan_contracts() -> Vec<RouteContract> {
    let request = json!({"type":"object","additionalProperties":false,"properties":{
        "operationId":uuid(),"name":{"type":"string","minLength":1,"maxLength":1000},
        "description":{"anyOf":[{"type":"string","maxLength":10000},{"type":"null"}]},
        "features":{"anyOf":[{"type":"array","maxItems":1000,"items":{"type":"string","maxLength":1000}},{"type":"null"}]},
        "active":{"type":"boolean"},"unitAmount":{"type":"integer","minimum":0,"maximum":9_007_199_254_740_991_u64},
        "currency":{"type":"string","pattern":"^[A-Z]{3}$"},"cadence":{"type":"string","enum":["day","week","month","year"]},
        "productId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_-]+$"}
        },"required":["operationId","name","active","unitAmount","currency","cadence","productId"]});
    let create=auth(RouteContract::post("Create Bound Plan","One durable operation UUID; direct authenticated creation with server-trusted products. Completed returns immutable IDs only, including duplicates.202 may already have provider/local effects; inspect, never automatically retry. Finite per-unit licensed Plan; no paid or entitlement proof.","/bound-plans"))
        .with_request_body(body("BoundPlanCreateRequest",&request))
        .with_response(200,Some(body("BoundPlanCompleted",&operation("completed"))))
        .with_response(202,Some(body("BoundPlanUncertain",&json!({"anyOf":[operation("unresolved"),operation("recordedEvidenceNoLongerCurrent")]}))));
    let mut parameter = FieldContract::required("operationId", "String");
    parameter.schema.json_schema = Some(uuid().to_string());
    let inspect=auth(RouteContract::get("Inspect Bound Plan Operation","Actor-owned historical journal status and IDs only; no URL/provider fields or current ownership/validity assertion. No redispatch authority.","/bound-plan-operations/:operationId"))
        .with_param(parameter).with_response(200,Some(body("BoundPlanOperation",&json!({"anyOf":[operation("pending"),operation("completed")]}))));
    vec![create, inspect]
}
