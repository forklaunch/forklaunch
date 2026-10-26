//! Local persistence protection, not rollback of already-applied provider effects.
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
    base.create_subscription(serde_json::from_value(json!({"id":"local-sub","partyId":"local-party","partyType":"USER","productId":"local-plan","active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"sub_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    (StripeBillingService::new(base, client), state, server)
}

fn bad_ids() -> Vec<Value> {
    vec![
        Value::Null,
        json!(7),
        json!({}),
        json!([]),
        json!(""),
        json!(" "),
        json!("\u{feff}"),
        json!("x\0y"),
        json!("x\u{85}y"),
        json!("x".repeat(1025)),
        json!("é".repeat(513)),
    ]
}
#[tokio::test]
async fn plan_consumed_product_is_validated_before_delete() {
    let (service, state, server) = fixture().await;
    let before =
        serde_json::to_value(service.base().get_plan("local-plan").await.unwrap()).unwrap();
    let mut invalid = bad_ids();
    invalid.extend(bad_ids().into_iter().map(|id| json!({"id":id})));
    for product in invalid {
        *state.result.lock().await = json!({"id":"plan_owned","object":"plan","product":product});
        let error = service
            .update_plan(serde_json::from_value(json!({"id":"local-plan"})).unwrap())
            .await
            .unwrap_err();
        assert_eq!(error.message, "Invalid Stripe billing response");
        assert_eq!(*state.calls.lock().await, ["GET /v1/plans/plan_owned"]);
        state.calls.lock().await.clear();
        assert_eq!(
            serde_json::to_value(service.base().get_plan("local-plan").await.unwrap()).unwrap(),
            before
        );
    }
    *state.result.lock().await = json!({"id":"plan_owned","object":"plan","product":{"id":"prod_owned"},"interval":"month","currency":"USD","amount":12});
    service
        .update_plan(serde_json::from_value(json!({"id":"local-plan"})).unwrap())
        .await
        .unwrap();
    assert_eq!(
        *state.calls.lock().await,
        [
            "GET /v1/plans/plan_owned",
            "DELETE /v1/plans/plan_owned",
            "POST /v1/plans",
            "GET /v1/plans/plan_new"
        ]
    );
    server.abort();
}
#[tokio::test]
async fn subscription_consumed_items_guard_and_override_precedence() {
    let (service, state, server) = fixture().await;
    let before =
        serde_json::to_value(service.base().get_subscription("local-sub").await.unwrap()).unwrap();
    let mut invalid = vec![
        Value::Null,
        json!({}),
        json!({"data":null}),
        json!({"data":[{}]}),
    ];
    invalid.extend(bad_ids().into_iter().map(|id| json!({"data":[{"id":id}]})));
    invalid.extend(
        bad_ids()
            .into_iter()
            .map(|id| json!({"data":[{"id":"si_owned","plan":{"id":id}}]})),
    );
    for items in invalid {
        *state.result.lock().await =
            json!({"id":"sub_owned","object":"subscription","items":items});
        let error = service
            .update_subscription(serde_json::from_value(json!({"id":"local-sub"})).unwrap())
            .await
            .unwrap_err();
        assert_eq!(error.message, "Invalid Stripe billing response");
        assert_eq!(
            *state.calls.lock().await,
            ["GET /v1/subscriptions/sub_owned"]
        );
        state.calls.lock().await.clear();
        assert_eq!(
            serde_json::to_value(service.base().get_subscription("local-sub").await.unwrap())
                .unwrap(),
            before
        );
    }
    let mut valid = vec![
        json!([]),
        json!([{"id":"si_owned"}]),
        json!([{"id":"si_owned","plan":null}]),
        json!([{"id":"si_owned","plan":{}}]),
    ];
    valid.extend(
        bad_ids()
            .into_iter()
            .map(|id| json!([{"id":"si_owned","plan":{"id":id}}])),
    );
    for (index, items) in valid.into_iter().enumerate() {
        *state.result.lock().await =
            json!({"id":"sub_owned","object":"subscription","items":{"data":items}});
        let mut request = json!({"id":"local-sub"});
        if index >= 4 {
            request["productId"] = json!("caller_override");
        }
        service
            .update_subscription(serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        assert_eq!(
            *state.calls.lock().await,
            [
                "GET /v1/subscriptions/sub_owned",
                "POST /v1/subscriptions/sub_owned"
            ]
        );
        state.calls.lock().await.clear();
    }
    server.abort();
}
