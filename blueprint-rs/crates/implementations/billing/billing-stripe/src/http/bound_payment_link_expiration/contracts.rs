//! Shared HTTP/MCP/artifact/SDK metadata, without server configuration or provider payloads.
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyContract, BodyKind, FieldContract, HmacAuth, RouteContract,
    SchemaRef,
};
use serde_json::{Value, json};
fn uuid() -> Value {
    json!({"type":"string","format":"uuid"})
}
fn parameter(name: &str) -> FieldContract {
    let mut f = FieldContract::required(name, "String");
    f.schema.json_schema = Some(uuid().to_string());
    f
}
fn body(name: &str, schema: &Value) -> BodyContract {
    let mut b = BodyContract::json(name);
    b.schema.json_schema = Some(schema.to_string());
    b
}
fn operation(states: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"required":["operationId","publicId","state"],"properties":{"operationId":uuid(),"publicId":uuid(),"state":{"type":"string","enum":states}}})
}
fn auth(route: RouteContract) -> RouteContract {
    let mut route = route
        .with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(
            HmacAuth::new().with_header_name("authorization"),
        ))
        .with_param(parameter("publicId"));
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
/// Explicit metadata-only expiration plus actor-owned historical IDs/status inspection.
/// Neither route deactivates Stripe URLs or establishes payment/entitlement state.
#[must_use]
pub fn bound_payment_link_expiration_contracts() -> Vec<RouteContract> {
    let expire=auth(RouteContract::post("Expire Bound Payment Link","Single-dispatch metadata.status=EXPIRED update followed by local access expiration. Does NOT set active=false or revoke the Stripe URL.202 requires inspection; no automatic retry.","/bound-payment-links/:publicId/expire"))
  .with_request_body(body("BoundPaymentLinkExpirationRequest",&json!({"type":"object","additionalProperties":false,"required":["operationId"],"properties":{"operationId":uuid()}})))
  .with_response(200,Some(body("BoundPaymentLinkExpirationCompleted",&operation(&["completed"]))))
  .with_response(202,Some(body("BoundPaymentLinkExpirationUncertain",&operation(&["pending","recordedEvidenceNoLongerCurrent"]))));
    let inspect=auth(RouteContract::get("Inspect Bound Payment Link Expiration","Actor-owned historical IDs/status only; no provider I/O or dispatch permission.404 is not rollback proof; completed does not imply a disabled provider URL.","/bound-payment-links/:publicId/expiration-operations/:operationId"))
  .with_param(parameter("operationId"))
  .with_response(200,Some(body("BoundPaymentLinkExpirationOperation",&operation(&["pending","completed"]))));
    vec![expire, inspect]
}

/// Publication-only event-linked recovery; this route never sends a provider POST.
#[must_use]
pub fn bound_payment_link_expiration_recovery_contracts() -> Vec<RouteContract> {
    let recovery=auth(RouteContract::post("Recover Bound Payment Link Expiration","Authenticated event/current reads finish only local metadata-expiration bookkeeping. No provider update, retry, URL revocation or payment-state claim.","/bound-payment-links/:publicId/expiration-operations/:operationId/recover"))
      .with_param(parameter("operationId"))
      .with_request_body(body("BoundPaymentLinkExpirationRecoveryRequest",&json!({"type":"object","additionalProperties":false,"required":["eventId"],"properties":{"eventId":{"type":"string","pattern":"^evt_[A-Za-z0-9_]+$","maxLength":128}}})))
      .with_response(200,Some(body("BoundPaymentLinkExpirationCompleted",&operation(&["completed"]))))
      .with_response(202,Some(body("BoundPaymentLinkExpirationUncertain",&operation(&["pending","recordedEvidenceNoLongerCurrent"]))));
    vec![
        recovery,
        bound_payment_link_expiration_contracts().remove(1),
    ]
}
