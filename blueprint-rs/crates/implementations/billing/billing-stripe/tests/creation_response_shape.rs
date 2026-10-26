//! Creation responses have no preexisting identity; validate shape, not ownership.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_interface::services::{
    BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService,
    SubscriptionService,
};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
#[derive(Clone, Default)]
struct Provider {
    result: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<String>>>,
}
async fn provider(State(state): State<Provider>, method: Method, uri: Uri) -> Json<Value> {
    state.calls.lock().await.push(method.to_string());
    Json(if uri.path() == "/v1/plans/plan_owned" {
        json!({"id":"plan_owned","object":"plan","product":"prod_owned","interval":"month","currency":"USD","amount":12})
    } else {
        state.result.lock().await.clone()
    })
}
struct Fixture {
    service: StripeBillingService<MemoryBillingStore>,
    store: Arc<MemoryBillingStore>,
    state: Provider,
    server: tokio::task::JoinHandle<()>,
}
async fn fixture() -> Fixture {
    let state = Provider::default();
    let app = Router::new().fallback(provider).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let store = Arc::new(MemoryBillingStore::new());
    let base = BaseBillingService::new(store.clone());
    base.create_plan(serde_json::from_value(json!({"id":"local-plan","name":"Owned","price":12,"cadence":"month","currency":"USD","active":true,"externalId":"plan_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    base.create_billing_portal_session(serde_json::from_value(json!({"id":"local-portal","customerId":"local-customer","uri":"https://example.test/old","expiresAt":"2099-01-01T00:00:00Z"})).unwrap()).await.unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 16384).unwrap(),
    )
    .unwrap();
    Fixture {
        service: StripeBillingService::new(base, client),
        store,
        state,
        server,
    }
}
async fn invoke(
    service: &StripeBillingService<MemoryBillingStore>,
    mode: &str,
) -> BlueprintResult<Value> {
    macro_rules! call {
        ($method:ident, $value:expr) => {
            serde_json::to_value(
                service
                    .$method(serde_json::from_value($value).unwrap())
                    .await?,
            )
            .unwrap()
        };
    }
    Ok(match mode {
        "plan" => call!(
            create_plan,
            json!({"id":"local-new","name":"Owned","price":12,"cadence":"month","currency":"USD","active":true,"externalId":"ignored","billingProvider":"ignored"})
        ),
        "subscription" => call!(
            create_subscription,
            json!({"id":"local-new","partyId":"local-customer","partyType":"USER","productId":"local-plan","active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"ignored","billingProvider":"ignored"})
        ),
        "checkout.session" => call!(
            create_checkout_session,
            json!({"customerId":"local-customer","currency":"USD","paymentMethods":["card"],"status":"PENDING","expiresAt":"2099-01-01T00:00:00Z"})
        ),
        "payment_link" => call!(
            create_payment_link,
            json!({"amount":12,"currency":"USD","paymentMethods":["card"],"status":"PENDING","providerFields":{"line_items":[]}})
        ),
        "billing_portal.session" => call!(
            create_billing_portal_session,
            json!({"customerId":"local-customer","expiresAt":"2099-01-01T00:00:00Z"})
        ),
        "plan-replace" => call!(update_plan, json!({"id":"local-plan"})),
        _ => call!(update_billing_portal_session, json!({"id":"local-portal"})),
    })
}
async fn snapshot(store: &MemoryBillingStore) -> Vec<Value> {
    let mut rows = Vec::new();
    for scope in [Scope::Durable, Scope::Cache] {
        for kind in [
            BillingKind::Plan,
            BillingKind::Subscription,
            BillingKind::CheckoutSession,
            BillingKind::PaymentLink,
            BillingKind::BillingPortal,
        ] {
            rows.push(json!(store.list(scope, kind, None).await.unwrap()));
        }
    }
    rows
}
#[tokio::test]
async fn all_creation_families_deny_malformed_response_before_storage() {
    for mode in [
        "plan",
        "subscription",
        "checkout.session",
        "payment_link",
        "billing_portal.session",
        "plan-replace",
        "portal-replace",
    ] {
        verify(mode).await;
    }
}
async fn verify(mode: &str) {
    let Fixture {
        service,
        store,
        state,
        server,
    } = fixture().await;
    let kind = match mode {
        "plan-replace" => "plan",
        "portal-replace" => "billing_portal.session",
        _ => mode,
    };
    let calls = if mode == "plan-replace" {
        vec!["GET", "DELETE", "POST"]
    } else {
        vec!["POST"]
    };
    let old = snapshot(&store).await;
    let base = json!({"id":"safe-custom-id","object":kind,"url":"https://example.test/new","product":{"id":"prod_owned","object":"product"}});
    let mut invalid = vec![Value::Null, json!([]), json!({})];
    for id in [
        Value::Null,
        json!(7),
        json!(""),
        json!("   "),
        json!("\u{feff}"),
        json!("bad\0id"),
        json!("bad\u{85}id"),
        json!("x".repeat(1025)),
        json!("é".repeat(513)),
    ] {
        let mut value = base.clone();
        value["id"] = id;
        invalid.push(value);
    }
    for object in [Value::Null, json!("customer")] {
        let mut value = base.clone();
        value["object"] = object;
        invalid.push(value);
    }
    for value in invalid {
        *state.result.lock().await = value;
        assert_eq!(
            invoke(&service, mode).await.unwrap_err().message,
            "Invalid Stripe billing response"
        );
        assert_eq!(*state.calls.lock().await, calls);
        state.calls.lock().await.clear();
        assert_eq!(snapshot(&store).await, old);
    }
    *state.result.lock().await = base;
    let result = invoke(&service, mode).await.unwrap();
    assert!(result.is_object());
    let mut successful = calls;
    if kind == "plan" {
        successful.push("GET");
    }
    assert_eq!(*state.calls.lock().await, successful);
    server.abort();
}
