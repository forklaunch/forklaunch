//! Read-only provider verification for interrupted checkout webhook completion.
use crate::{
    client::{StripeBillingClient, StripeResource},
    services::StripeBillingService,
};
use forklaunch_billing_base::services::postgres::{PendingWebhook, PostgresBillingStore};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};

impl StripeBillingService<PostgresBillingStore> {
    /// Repairs local completion only after Stripe's event and current session prove
    /// the corresponding metadata mutation already completed. Performs two GETs,
    /// never repeats metadata POST/capture/refund or claims funds have been paid.
    /// Stripe event retrieval is limited to 30 days; unavailable events and missing
    /// local ownership remain manual review. Connected-account events require a
    /// separately scoped workflow and are rejected by this account-local API.
    /// # Errors
    /// Rejects substituted/nonterminal/inconsistent evidence, missing ownership,
    /// active/stale attempts, concurrent local changes and atomic audit failures.
    pub async fn reconcile_checkout_webhook(
        &self,
        pending: &PendingWebhook,
    ) -> BlueprintResult<()> {
        let (session_id, customer_id) = verified_provider_session(self.client(), pending).await?;
        let snapshot = self
            .base()
            .store()
            .snapshot_verified_checkout_repair(
                pending,
                &session_id,
                &customer_id,
                self.base().database_backup_enabled(),
            )
            .await?;
        self.base()
            .store()
            .complete_verified_checkout_repair(snapshot)
            .await
    }
}
async fn verified_provider_session(
    client: &StripeBillingClient,
    pending: &PendingWebhook,
) -> BlueprintResult<(String, String)> {
    target(pending)?;
    let event = client
        .retrieve(StripeResource::Event, &pending.event_id, &json!({}))
        .await?;
    let (session_id, customer_id) = event_identity(pending, &event)?;
    let current = client
        .retrieve(StripeResource::CheckoutSession, session_id, &json!({}))
        .await?;
    verify_session(pending, &event, &current)?;
    Ok((session_id.into(), customer_id.into()))
}
fn required() -> BlueprintError {
    BlueprintError::new(
        "billing_checkout_reconciliation_required",
        "Checkout provider evidence requires manual review",
    )
}
fn target(pending: &PendingWebhook) -> BlueprintResult<(&'static str, &'static str)> {
    if pending.provider != "stripe" {
        return Err(required());
    }
    match pending.event_type.as_str() {
        "checkout.session.completed" => Ok(("complete", "SUCCESS")),
        "checkout.session.expired" => Ok(("expired", "FAILED")),
        _ => Err(required()),
    }
}
fn identifier(value: &Value) -> BlueprintResult<&str> {
    value
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 1024 && !id.chars().any(char::is_control))
        .ok_or_else(required)
}
fn customer(value: &Value) -> BlueprintResult<&str> {
    identifier(value).or_else(|_| identifier(&value["id"]))
}
fn event_identity<'a>(
    pending: &PendingWebhook,
    event: &'a Value,
) -> BlueprintResult<(&'a str, &'a str)> {
    let (terminal, _) = target(pending)?;
    if event["id"] != pending.event_id
        || event["object"] != "event"
        || event["type"] != pending.event_type
        || event["livemode"].as_bool().is_none()
        || ["account", "context"]
            .iter()
            .any(|key| event.get(key).is_some_and(|value| !value.is_null()))
    {
        return Err(required());
    }
    let object = &event["data"]["object"];
    if object["object"] != "checkout.session"
        || object["status"] != terminal
        || object["mode"] != "subscription"
    {
        return Err(required());
    }
    Ok((identifier(&object["id"])?, customer(&object["customer"])?))
}
fn verify_session(pending: &PendingWebhook, event: &Value, current: &Value) -> BlueprintResult<()> {
    let (session_id, customer_id) = event_identity(pending, event)?;
    let (terminal, metadata) = target(pending)?;
    if current["id"] != session_id
        || current["object"] != "checkout.session"
        || current["mode"] != "subscription"
        || current["status"] != terminal
        || current["metadata"]["status"] != metadata
        || current["livemode"] != event["livemode"]
        || customer(&current["customer"])? != customer_id
    {
        return Err(required());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures(expired: bool) -> (PendingWebhook, Value, Value) {
        let pending = PendingWebhook {
            provider: "stripe".into(),
            event_id: "evt_owned".into(),
            event_type: if expired {
                "checkout.session.expired"
            } else {
                "checkout.session.completed"
            }
            .into(),
            attempt_id: "a".into(),
            started_at: "unused".into(),
        };
        let current = json!({"id":"cs_owned", "object":"checkout.session", "customer":"cus_owned", "mode":"subscription", "status":if expired {"expired"} else {"complete"}, "livemode":false,"metadata":{"status":if expired {"FAILED"} else {"SUCCESS"}},"payment_status":"unpaid"});
        let event = json!({"id":"evt_owned","object":"event","type":pending.event_type,"livemode":false,"data":{"object":current}});
        (pending, event, current)
    }
    #[test]
    fn authenticated_event_and_session_evidence_is_exact_and_nonfinancial() {
        for expired in [false, true] {
            let (pending, event, current) = fixtures(expired);
            verify_session(&pending, &event, &current).unwrap();
            for (pointer, value) in [
                ("/id", json!("cs_other")),
                ("/object", json!("payment_intent")),
                ("/customer", json!("cus_other")),
                ("/status", json!("open")),
                ("/mode", json!("payment")),
                ("/livemode", json!(true)),
                ("/metadata/status", json!("PENDING")),
                ("/metadata", json!(null)),
            ] {
                let mut invalid = current.clone();
                *invalid.pointer_mut(pointer).unwrap() = value;
                assert!(verify_session(&pending, &event, &invalid).is_err());
            }
            for (pointer, value) in [
                ("/id", json!("evt_other")),
                ("/type", json!("unknown")),
                ("/object", json!("customer")),
                ("/livemode", json!(null)),
                ("/data/object/status", json!("open")),
                ("/data/object/customer", json!(null)),
            ] {
                let mut invalid = event.clone();
                *invalid.pointer_mut(pointer).unwrap() = value;
                assert!(verify_session(&pending, &invalid, &current).is_err());
            }
            for key in ["account", "context"] {
                let mut invalid = event.clone();
                invalid[key] = json!("acct_other");
                assert!(verify_session(&pending, &invalid, &current).is_err());
            }
        }
    }
    #[tokio::test]
    async fn provider_repair_uses_only_authenticated_gets() {
        use axum::{Json, Router, http::HeaderMap, routing::get};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let (pending, event, current) = fixtures(false);
        let calls = Arc::new(AtomicUsize::new(0));
        let first = calls.clone();
        let second = calls.clone();
        let app = Router::new()
            .route(
                "/v1/events/evt_owned",
                get(move |headers: HeaderMap| {
                    let body = event.clone();
                    let calls = first.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer owned-secret");
                        calls.fetch_add(1, Ordering::SeqCst);
                        Json(body)
                    }
                }),
            )
            .route(
                "/v1/checkout/sessions/cs_owned",
                get(move |headers: HeaderMap| {
                    let body = current.clone();
                    let calls = second.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer owned-secret");
                        calls.fetch_add(1, Ordering::SeqCst);
                        Json(body)
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let http = forklaunch_core::http::provider_http::ProviderHttpClient::new(
            &format!("http://{address}"),
            std::time::Duration::from_secs(5),
            1024 * 1024,
        )
        .unwrap();
        let client = StripeBillingClient::with_transport("owned-secret".into(), http).unwrap();
        assert_eq!(
            verified_provider_session(&client, &pending).await.unwrap(),
            ("cs_owned".into(), "cus_owned".into())
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        server.abort();
    }
}
