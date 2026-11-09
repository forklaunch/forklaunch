//! Local persistence protection, not rollback of already-applied provider effects.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{BaseBillingService, memory::MemoryBillingStore};
use forklaunch_billing_interface::services::{PlanService, SubscriptionService};
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
    state
        .calls
        .lock()
        .await
        .push(format!("{method} {}", uri.path()));
    Json(match (method.as_str(), uri.path()) {
        ("GET", "/v1/plans/plan_owned") => {
            json!({"id":"plan_owned","object":"plan","product":"prod_owned","interval":"month","currency":"USD","amount":12})
        }
        ("DELETE", "/v1/plans/plan_owned") => json!({"id":"plan_owned","deleted":true}),
        ("POST", "/v1/plans") => json!({"id":"plan_new","object":"plan"}),
        ("GET", "/v1/subscriptions/sub_owned") => {
            json!({"id":"sub_owned","object":"subscription","items":{"data":[{"id":"si_owned","plan":{"id":"plan_owned"}}]}})
        }
        ("GET", "/v1/plans/plan_new") | ("POST", "/v1/subscriptions/sub_owned") => {
            state.result.lock().await.clone()
        }
        _ => panic!("Unexpected retry, compensation or provider request"),
    })
}
async fn fixture() -> (
    StripeBillingService<MemoryBillingStore>,
    Provider,
    tokio::task::JoinHandle<()>,
) {
    let state = Provider::default();
    let app = Router::new().fallback(provider).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = BaseBillingService::new(Arc::new(MemoryBillingStore::new()));
    base.create_plan(serde_json::from_value(json!({"id":"local-plan","name":"Owned","price":12,"cadence":"month","currency":"USD","active":true,"externalId":"plan_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    base.create_subscription(serde_json::from_value(json!({"id":"local-sub","partyId":"local-party","partyType":"USER","productId":"local-plan","active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"sub_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    (StripeBillingService::new(base, client), state, server)
}
async fn invoke(
    service: &StripeBillingService<MemoryBillingStore>,
    mode: &str,
) -> BlueprintResult<Value> {
    Ok(match mode {
        "create" => serde_json::to_value(service.create_plan(serde_json::from_value(json!({"id":"local-new","name":"Owned","price":12,"cadence":"month","currency":"USD","active":true,"externalId":"ignored","billingProvider":"ignored"})).unwrap()).await?).unwrap(),
        "replace" => serde_json::to_value(service.update_plan(serde_json::from_value(json!({"id":"local-plan"})).unwrap()).await?).unwrap(),
        _ => serde_json::to_value(service.update_subscription(serde_json::from_value(json!({"id":"local-sub"})).unwrap()).await?).unwrap(),
    })
}
#[tokio::test]
async fn plan_create_mismatch_never_persists_or_retries() {
    verify_boundary("create").await;
}
#[tokio::test]
async fn plan_replacement_mismatch_never_persists_or_retries() {
    verify_boundary("replace").await;
}
#[tokio::test]
async fn subscription_update_mismatch_never_persists_or_retries() {
    verify_boundary("subscription").await;
}
async fn verify_boundary(mode: &str) {
    let (service, state, server) = fixture().await;
    let old_plan =
        serde_json::to_value(service.base().get_plan("local-plan").await.unwrap()).unwrap();
    let old_sub =
        serde_json::to_value(service.base().get_subscription("local-sub").await.unwrap()).unwrap();
    let (id, kind, calls) = match mode {
        "create" => (
            "plan_new",
            "plan",
            vec!["POST /v1/plans", "GET /v1/plans/plan_new"],
        ),
        "replace" => (
            "plan_new",
            "plan",
            vec![
                "GET /v1/plans/plan_owned",
                "DELETE /v1/plans/plan_owned",
                "POST /v1/plans",
                "GET /v1/plans/plan_new",
            ],
        ),
        _ => (
            "sub_owned",
            "subscription",
            vec![
                "GET /v1/subscriptions/sub_owned",
                "POST /v1/subscriptions/sub_owned",
            ],
        ),
    };
    for value in [
        json!({"id":"private_wrong","object":kind}),
        json!({"object":kind}),
        json!({"id":null,"object":kind}),
        json!({"id":7,"object":kind}),
        json!({"id":id,"object":"customer"}),
        json!({"id":id}),
        json!({"id":id,"object":null}),
        Value::Null,
        json!([]),
    ] {
        *state.result.lock().await = value;
        let error = invoke(&service, mode).await.unwrap_err();
        assert_eq!(error.message, "Invalid Stripe billing response");
        assert_eq!(*state.calls.lock().await, calls);
        state.calls.lock().await.clear();
        assert_eq!(
            serde_json::to_value(service.base().get_plan("local-plan").await.unwrap()).unwrap(),
            old_plan
        );
        assert_eq!(
            serde_json::to_value(service.base().get_subscription("local-sub").await.unwrap())
                .unwrap(),
            old_sub
        );
        assert!(service.base().get_plan("local-new").await.is_err());
    }
    *state.result.lock().await =
        json!({"id":id,"object":kind,"product":{"id":"prod_owned","object":"product"}});
    let value = invoke(&service, mode).await.unwrap();
    assert_eq!(value["externalId"], id);
    assert_eq!(
        value["id"],
        match mode {
            "create" => "local-new",
            "replace" => "local-plan",
            _ => "local-sub",
        }
    );
    assert_eq!(*state.calls.lock().await, calls);
    server.abort();
}
