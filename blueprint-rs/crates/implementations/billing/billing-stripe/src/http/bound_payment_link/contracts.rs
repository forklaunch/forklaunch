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
pub fn bound_payment_link_contracts() -> Vec<RouteContract> {
    let request = json!({"type":"object","additionalProperties":false,"properties":{
        "operationId":uuid(),"amount":{"type":"number","minimum":0,"description":"Declared local value only, not independently verified Stripe price/charged total."},
        "currency":{"type":"string","pattern":"^[A-Z]{3}$"},
        "paymentMethods":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_]+$"}},
        "lineItems":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"object","additionalProperties":false,"properties":{"price":{"type":"string","minLength":1,"maxLength":128},"quantity":{"type":"integer","minimum":1,"maximum":999_999}},"required":["price","quantity"]}}
        },"required":["operationId","amount","currency","paymentMethods","lineItems"]});
    let create=auth(RouteContract::post("Create Bound Payment Link","One durable operation UUID; direct authenticated creation with server-trusted prices. Completed returns immutable IDs only, including duplicates.202 may already have provider/local effects; inspect, never automatically retry. No paid/entitlement or charged-total proof.","/bound-payment-links"))
        .with_request_body(body("BoundPaymentLinkCreateRequest",&request))
        .with_response(200,Some(body("BoundPaymentLinkCompleted",&operation("completed"))))
        .with_response(202,Some(body("BoundPaymentLinkUncertain",&json!({"anyOf":[operation("unresolved"),operation("recordedEvidenceNoLongerCurrent")]}))));
    let mut parameter = FieldContract::required("operationId", "String");
    parameter.schema.json_schema = Some(uuid().to_string());
    let inspect=auth(RouteContract::get("Inspect Bound Payment Link Operation","Actor-owned historical journal status and IDs only; no URL/provider fields or current ownership/validity assertion. No redispatch authority.","/bound-payment-link-operations/:operationId"))
        .with_param(parameter).with_response(200,Some(body("BoundPaymentLinkOperation",&json!({"anyOf":[operation("pending"),operation("completed")]}))));
    vec![create, inspect]
}
