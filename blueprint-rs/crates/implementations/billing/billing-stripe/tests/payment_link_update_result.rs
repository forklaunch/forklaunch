//! Provider POST may already have applied; only local persistence is guarded.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{BaseBillingService, memory::MemoryBillingStore};
use forklaunch_billing_interface::services::PaymentLinkService;
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
    Json(state.result.lock().await.clone())
}
#[tokio::test]
async fn payment_link_result_cannot_redirect_or_corrupt_local_update() {
    let state = Provider::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().fallback(provider).with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = BaseBillingService::new(Arc::new(MemoryBillingStore::new()));
    for id in ["plink_owned", "plink_other"] {
        base.create_payment_link(serde_json::from_value(json!({"id":id,"amount":12,"currency":"USD","paymentMethods":["card"],"status":"PENDING"})).unwrap()).await.unwrap();
    }
    let old = base.get_payment_link("plink_owned").await.unwrap();
    let other = base.get_payment_link("plink_other").await.unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    let service = StripeBillingService::new(base, client);
    for result in [
        json!({"id":"plink_other","object":"payment_link"}),
        json!({"object":"payment_link"}),
        json!({"id":null,"object":"payment_link"}),
        json!({"id":7,"object":"payment_link"}),
        json!({"id":"plink_owned","object":"price"}),
        json!({"id":"plink_owned"}),
        json!({"id":"plink_owned","object":"payment_link","line_items":{"data":null}}),
        json!({"id":"plink_owned","object":"payment_link","line_items":{"data":[{}]}}),
        json!({"id":"plink_owned","object":"payment_link","line_items":{"data":[{"amount_total":"12"}]}}),
        json!({"id":"plink_owned","object":"payment_link","line_items":{"data":[{"amount_total":f64::MAX},{"amount_total":f64::MAX}]}}),
    ] {
        *state.result.lock().await = result;
        let error = service
            .update_payment_link(serde_json::from_value(json!({"id":"plink_owned"})).unwrap())
            .await
            .unwrap_err();
        assert_eq!(error.message, "Invalid Stripe billing response");
        assert_eq!(
            *state.calls.lock().await,
            ["POST /v1/payment_links/plink_owned"]
        );
        state.calls.lock().await.clear();
        assert_eq!(
            serde_json::to_value(
                service
                    .base()
                    .get_payment_link("plink_owned")
                    .await
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        assert_eq!(
            serde_json::to_value(
                service
                    .base()
                    .get_payment_link("plink_other")
                    .await
                    .unwrap()
            )
            .unwrap(),
            serde_json::to_value(&other).unwrap()
        );
    }
    *state.result.lock().await = json!({"id":"plink_owned","object":"payment_link","line_items":{"data":[{"amount_total":12},{"amount_total":3}]}});
    let result = service
        .update_payment_link(serde_json::from_value(json!({"id":"plink_owned"})).unwrap())
        .await
        .unwrap();
    assert_eq!(result.id, "plink_owned");
    assert!((result.amount - 15.0).abs() < f64::EPSILON);
    assert_eq!(
        *state.calls.lock().await,
        ["POST /v1/payment_links/plink_owned"]
    );
    server.abort();
}
