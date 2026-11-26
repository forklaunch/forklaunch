//! Configured mapper preflight parity; generic/custom behavior remains explicit.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{BaseBillingService, memory::MemoryBillingStore};
use forklaunch_billing_interface::services::{PlanService, SubscriptionService};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
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
        ("GET", "/v1/plans/plan_owned" | "/v1/subscriptions/sub_owned") => {
            state.result.lock().await.clone()
        }
        ("DELETE", "/v1/plans/plan_owned") => json!({"id":"plan_owned","deleted":true}),
        ("POST", "/v1/plans") => json!({"id":"plan_new","object":"plan"}),
        ("GET", "/v1/plans/plan_new") => {
            json!({"id":"plan_new","object":"plan","product":{"id":"prod_owned","object":"product"}})
        }
        ("POST", "/v1/subscriptions/sub_owned") => {
            json!({"id":"sub_owned","object":"subscription"})
        }
        _ => panic!("Unexpected provider request"),
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
    base.create_subscription(serde_json::from_value(json!({"id":"local-sub","partyId":"local-party","partyType":"user","productId":"local-plan","active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"sub_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    (StripeBillingService::new(base, client), state, server)
}

#[tokio::test]
async fn configured_owner_preflight_matches_ts_before_provider_io() {
    let (service, state, server) = fixture().await;
    let service = service.with_configured_subscription_updates(true);
    let before = service.base().get_subscription("local-sub").await.unwrap();
    for patch in [
        json!({"partyId":"other"}),
        json!({"partyType":"organization"}),
        json!({"partyId":""}),
        json!({"billingProvider":"invalid"}),
        json!({"partyType":"invalid"}),
    ] {
        let mut input = patch;
        input["id"] = json!("local-sub");
        let error = service
            .update_subscription(serde_json::from_value(input).unwrap())
            .await
            .unwrap_err();
        assert_eq!(error.code, "billing_configured_mapper_error");
        assert!(state.calls.lock().await.is_empty());
        assert_eq!(
            service.base().get_subscription("local-sub").await.unwrap(),
            before
        );
    }
    let missing = service
        .update_subscription(
            serde_json::from_value(json!({"id":"missing","partyId":"other"})).unwrap(),
        )
        .await
        .unwrap_err();
    assert_ne!(
        missing.message,
        "Subscription ownership cannot change during an update"
    );
    assert!(state.calls.lock().await.is_empty());
    for patch in [
        json!({}),
        json!({"partyId":"local-party","partyType":"user"}),
    ] {
        *state.result.lock().await = json!({"id":"sub_owned","object":"subscription","items":{"data":[{"id":"si_owned","plan":{"id":"plan_owned"}}]}});
        let mut input = patch;
        input["id"] = json!("local-sub");
        let updated = service
            .update_subscription(serde_json::from_value(input).unwrap())
            .await
            .unwrap();
        assert_eq!(updated.party_id, "local-party");
        assert_eq!(updated.party_type, "user");
        assert_eq!(
            *state.calls.lock().await,
            [
                "GET /v1/subscriptions/sub_owned",
                "POST /v1/subscriptions/sub_owned"
            ]
        );
        state.calls.lock().await.clear();
    }
    for party in ["\u{85}", "\u{feff}", "\u{2028}"] {
        service
            .base()
            .update_subscription(
                serde_json::from_value(json!({"id":"local-sub","partyId":party})).unwrap(),
            )
            .await
            .unwrap();
        let result = service
            .update_subscription(serde_json::from_value(json!({"id":"local-sub"})).unwrap())
            .await;
        if party == "\u{85}" {
            assert_eq!(result.unwrap().party_id, party);
            assert_eq!(
                *state.calls.lock().await,
                [
                    "GET /v1/subscriptions/sub_owned",
                    "POST /v1/subscriptions/sub_owned"
                ]
            );
        } else {
            assert_eq!(result.unwrap_err().code, "billing_configured_mapper_error");
            assert!(state.calls.lock().await.is_empty());
        }
        state.calls.lock().await.clear();
    }
    for field in ["partyId", "partyType"] {
        let mut input = json!({"id":"local-sub"});
        input[field] = Value::Null;
        assert!(serde_json::from_value::<forklaunch_billing_interface::services::UpdateSubscriptionDto>(input).is_err());
    }
    assert!(state.calls.lock().await.is_empty());
    server.abort();
}

#[tokio::test]
async fn generic_custom_update_still_allows_party_patch() {
    let (service, state, server) = fixture().await;
    *state.result.lock().await =
        json!({"id":"sub_owned","object":"subscription","items":{"data":[]}});
    let updated = service
        .update_subscription(
            serde_json::from_value(json!({"id":"local-sub","partyId":"other"})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.party_id, "other");
    assert_eq!(
        *state.calls.lock().await,
        [
            "GET /v1/subscriptions/sub_owned",
            "POST /v1/subscriptions/sub_owned"
        ]
    );
    server.abort();
}
