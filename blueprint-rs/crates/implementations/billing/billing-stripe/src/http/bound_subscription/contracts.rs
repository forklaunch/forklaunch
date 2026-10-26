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
pub fn bound_subscription_contracts() -> Vec<RouteContract> {
    let request = json!({"type":"object","additionalProperties":false,"properties":{
        "operationId":uuid(),"localCustomerId":uuid(),"localPlanId":uuid(),
        "quantity":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_u64},
        "description":{"anyOf":[{"type":"string","maxLength":500,"description":"At most 500 UTF-8 bytes; NUL is unsupported."},{"type":"null"}]}
        },"required":["operationId","localCustomerId","localPlanId","quantity"]});
    let create=auth(RouteContract::post("Create Bound Subscription","One durable operation UUID; direct authenticated creation with registered Customer and completed server-trusted Plan provenance, a separate publisher and explicit internal service-level tenant authority. Completed returns immutable IDs only, including duplicates.202 may already have provider/local effects; inspect, never automatically retry. Automatic default-incomplete collection only; no end-user/Party ownership, paid or entitlement proof.","/bound-subscriptions"))
        .with_request_body(body("BoundSubscriptionCreateRequest",&request))
        .with_response(200,Some(body("BoundSubscriptionCompleted",&operation("completed"))))
        .with_response(202,Some(body("BoundSubscriptionUncertain",&json!({"anyOf":[operation("unresolved"),operation("recordedEvidenceNoLongerCurrent")]}))));
    let mut parameter = FieldContract::required("operationId", "String");
    parameter.schema.json_schema = Some(uuid().to_string());
    let inspect=auth(RouteContract::get("Inspect Bound Subscription Operation","Actor-owned historical journal status and IDs only; no URL/provider fields or current ownership/validity assertion. No redispatch authority.","/bound-subscription-operations/:operationId"))
        .with_param(parameter).with_response(200,Some(body("BoundSubscriptionOperation",&json!({"anyOf":[operation("pending"),operation("completed")]}))));
    vec![create, inspect]
}

/// Explicit GET-only-provider recovery; local publication still requires a separate publisher.
/// # Panics
/// Only invalid package-owned static metadata can panic.
#[must_use]
pub fn bound_subscription_recovery_contracts() -> Vec<RouteContract> {
    let mut parameter = FieldContract::required("operationId", "String");
    parameter.schema.json_schema = Some(uuid().to_string());
    let recovery=auth(RouteContract::post("Recover Bound Subscription Operation","Publish only an existing same-actor pending intent from exact authenticated creation Event request-key linkage and current Customer/Plan/Subscription evidence. No provider POST, retry or process-origin/payment proof. Restricted reservation window and rendering API version apply. Completed returns historical IDs only;202 requires receipt inspection.","/bound-subscription-operations/:operationId/recover"))
        .with_param(parameter)
        .with_request_body(body("BoundSubscriptionRecoveryRequest",&json!({"type":"object","additionalProperties":false,"properties":{"eventId":{"type":"string","pattern":"^evt_[A-Za-z0-9_-]*$","maxLength":128}},"required":["eventId"]})))
        .with_response(200,Some(body("BoundSubscriptionCompleted",&operation("completed"))))
        .with_response(202,Some(body("BoundSubscriptionUncertain",&json!({"anyOf":[operation("unresolved"),operation("recordedEvidenceNoLongerCurrent")]}))));
    let inspection = bound_subscription_contracts()
        .pop()
        .expect("static inspection contract");
    vec![recovery, inspection]
}
