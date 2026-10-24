//! A deliberate known-gap regression: legacy services do not resolve local UUIDs.
use axum::{Router, extract::State, http::StatusCode, routing::post};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_interface::services::SubscriptionService;
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
#[tokio::test]
async fn legacy_subscription_sends_unresolved_local_uuid_targets() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route(
            "/v1/subscriptions",
            post(
                |State(calls): State<Arc<Mutex<Vec<String>>>>, body: String| async move {
                    calls.lock().unwrap().push(body);
                    (
                        StatusCode::BAD_REQUEST,
                        "strict provider rejects local UUID targets",
                    )
                },
            ),
        )
        .with_state(calls.clone());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let store = Arc::new(MemoryBillingStore::new());
    let service = StripeBillingService::new(
        BaseBillingService::new(store.clone()),
        StripeBillingClient::with_transport(
            "fixture".into(),
            ProviderHttpClient::new(&url, Duration::from_secs(2), 16384).unwrap(),
        )
        .unwrap(),
    );
    let party = uuid::Uuid::new_v4().to_string();
    let plan = uuid::Uuid::new_v4().to_string();
    let dto=serde_json::from_value(json!({"partyId":party,"partyType":"user","productId":plan,"active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"ignored","billingProvider":"stripe"})).unwrap();
    assert!(service.create_subscription(dto).await.is_err());
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let fields: std::collections::HashMap<_, _> =
        reqwest::Url::parse(&format!("http://fixture/?{}", calls[0]))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    assert_eq!(fields["customer"], party);
    assert_eq!(fields["items[0][plan]"], plan);
    drop(calls);
    assert!(
        store
            .list(Scope::Durable, BillingKind::Subscription, None)
            .await
            .unwrap()
            .is_empty()
    );
    task.abort();
}
