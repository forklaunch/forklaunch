//! Configured cache/patch preflight, then dispatch to the stored Stripe id.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_interface::services::{PaymentLinkService, UpdatePaymentLinkDto};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
#[derive(Clone, Default)]
struct Provider {
    calls: Arc<Mutex<Vec<String>>>,
}
async fn provider(State(state): State<Provider>, method: Method, uri: Uri) -> Json<Value> {
    state
        .calls
        .lock()
        .await
        .push(format!("{method} {}", uri.path()));
    Json(
        json!({"id":uri.path().rsplit('/').next().unwrap(),"object":"payment_link","line_items":{"data":[{"amount_total":34}]}}),
    )
}
fn record() -> Value {
    json!({"id":"public-id","amount":12,"currency":"USD","status":"PENDING","paymentMethods":["card"],"providerFields":{"id":"plink_stored_reference","object":"payment_link","livemode":false}})
}
fn patch(value: Value) -> UpdatePaymentLinkDto {
    let mut value = value;
    value["id"] = json!("public-id");
    serde_json::from_value(value).unwrap()
}
struct RawCache(Option<Value>);
impl BillingStore for RawCache {
    async fn get(&self, _: Scope, _: BillingKind, _: &str) -> BlueprintResult<Option<Value>> {
        Ok(self.0.clone())
    }
    async fn insert(
        &self,
        _: Scope,
        _: BillingKind,
        _: Value,
        _: Duration,
    ) -> BlueprintResult<Value> {
        panic!("unexpected write")
    }
    async fn patch(
        &self,
        _: Scope,
        _: BillingKind,
        _: &str,
        _: Value,
        _: Duration,
    ) -> BlueprintResult<Value> {
        panic!("unexpected write")
    }
    async fn delete(&self, _: Scope, _: BillingKind, _: &str) -> BlueprintResult<bool> {
        panic!("unexpected write")
    }
    async fn list(
        &self,
        _: Scope,
        _: BillingKind,
        _: Option<&[String]>,
    ) -> BlueprintResult<Vec<Value>> {
        panic!("unexpected list")
    }
    async fn find_active_subscription(&self, _: &str, _: &str) -> BlueprintResult<Option<Value>> {
        panic!("unrelated read")
    }
    async fn find_subscription_id_by_external_id(
        &self,
        _: &str,
    ) -> BlueprintResult<Option<String>> {
        panic!("unrelated read")
    }
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn configured_payment_link_update_preflight_and_generic_sequence() {
    let state = Provider::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().fallback(provider).with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    for change in [
        None,
        Some(json!({"id":"other"})),
        Some(json!({"id":" "})),
        Some(json!({"status":"INVALID"})),
        Some(json!({"amount":"bad"})),
        Some(json!({"createdAt":"bad-date"})),
    ] {
        let value = change.map(|change| {
            let mut value = record();
            value
                .as_object_mut()
                .unwrap()
                .extend(change.as_object().unwrap().clone());
            value
        });
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(RawCache(value))),
            client.clone(),
        )
        .with_configured_payment_link_updates(true);
        assert!(
            service
                .update_payment_link(patch(json!({"amount":34})))
                .await
                .is_err()
        );
        assert!(state.calls.lock().await.is_empty());
    }
    let store = Arc::new(MemoryBillingStore::new());
    let original = record();
    store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            original.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let service = StripeBillingService::new(BaseBillingService::new(store.clone()), client.clone())
        .with_configured_payment_link_updates(true);
    for input in [
        json!({"status":"INVALID"}),
        json!({"currency":"INVALID"}),
        json!({"paymentMethods":["invalid"]}),
    ] {
        let input = patch(input);
        let before = input.clone();
        assert_eq!(
            service.update_payment_link(input).await.unwrap_err().code,
            "billing_configured_mapper_error"
        );
        assert!(state.calls.lock().await.is_empty());
        assert_eq!(
            store
                .get(Scope::Cache, BillingKind::PaymentLink, "public-id")
                .await
                .unwrap(),
            Some(original.clone())
        );
        assert_eq!(before.id, "public-id");
    }
    let mut input = patch(json!({}));
    input.amount = Some(f64::NAN);
    assert!(service.update_payment_link(input).await.is_err());
    assert!(state.calls.lock().await.is_empty());
    // A record without a stored Stripe id fails before any Stripe call rather
    // than sending Stripe the public id.
    let mut no_reference = original.clone();
    no_reference
        .as_object_mut()
        .unwrap()
        .remove("providerFields");
    store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            no_reference,
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert_eq!(
        service
            .update_payment_link(patch(json!({"status":"COMPLETED"})))
            .await
            .unwrap_err()
            .code,
        "billing_provider_reference_missing"
    );
    assert!(state.calls.lock().await.is_empty());
    store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            original.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let updated = service
        .update_payment_link(patch(json!({"status":"COMPLETED"})))
        .await
        .unwrap();
    assert_eq!(updated.id, "public-id");
    assert_eq!(updated.status, "COMPLETED");
    assert_eq!(updated.amount.to_bits(), 34.0_f64.to_bits());
    assert_eq!(
        *state.calls.lock().await,
        vec!["POST /v1/payment_links/plink_stored_reference"]
    );
    for amount in [f64::INFINITY, f64::NEG_INFINITY] {
        state.calls.lock().await.clear();
        let mut input = patch(json!({}));
        input.amount = Some(amount);
        let result = service.update_payment_link(input).await.unwrap();
        assert_eq!(result.amount.to_bits(), 34.0_f64.to_bits());
        assert_eq!(state.calls.lock().await.len(), 1);
    }
    // Generic/custom also resolves the stored Stripe id first, so a missing row
    // fails before any Stripe call.
    state.calls.lock().await.clear();
    let generic = StripeBillingService::new(
        BaseBillingService::new(Arc::new(MemoryBillingStore::new())),
        client.clone(),
    );
    assert_eq!(
        generic
            .update_payment_link(patch(json!({})))
            .await
            .unwrap_err()
            .code,
        "billing_not_found"
    );
    assert!(state.calls.lock().await.is_empty());
    // Read selection does not silently grant the new update preflight behavior.
    state.calls.lock().await.clear();
    let read_only = StripeBillingService::new(
        BaseBillingService::new(Arc::new(MemoryBillingStore::new())),
        client,
    )
    .with_configured_cached_reads(true);
    assert!(
        read_only
            .update_payment_link(patch(json!({})))
            .await
            .is_err()
    );
    assert!(state.calls.lock().await.is_empty());
    server.abort();
}
