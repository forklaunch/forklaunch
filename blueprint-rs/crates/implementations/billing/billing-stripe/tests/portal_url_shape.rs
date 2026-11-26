//! Provider POST may already have applied; only local persistence is guarded.
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use forklaunch_billing_base::services::{BaseBillingService, memory::MemoryBillingStore};
use forklaunch_billing_interface::services::BillingPortalService;
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
async fn fixture() -> (
    StripeBillingService<MemoryBillingStore>,
    Provider,
    Value,
    tokio::task::JoinHandle<()>,
) {
    let state = Provider::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().fallback(provider).with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let base = BaseBillingService::new(Arc::new(MemoryBillingStore::new()));
    base.create_billing_portal_session(serde_json::from_value(json!({"id":"bps_old","customerId":"cus_owned","uri":"https://example.test/old","expiresAt":"2026-01-01T00:00:00Z","providerFields":{"id":"bps_old"}})).unwrap()).await.unwrap();
    let old =
        serde_json::to_value(base.get_billing_portal_session("bps_old").await.unwrap()).unwrap();
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    (StripeBillingService::new(base, client), state, old, server)
}
#[tokio::test]
async fn portal_url_create_and_replacement_guard_local_state_after_provider_post() {
    let (service, state, old, server) = fixture().await;
    for replace in [false, true] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!(1)),
            Some(json!(true)),
            Some(json!({})),
            Some(json!([])),
            Some(json!("")),
            Some(json!("   ")),
            Some(json!("\t\n")),
            Some(json!("\u{feff}")),
        ] {
            let mut response = json!({"id":"bps_new","object":"billing_portal.session"});
            if let Some(value) = value {
                response["url"] = value;
            }
            *state.result.lock().await = response;
            let result = if replace {
                service
                    .update_billing_portal_session(
                        serde_json::from_value(json!({"id":"bps_old"})).unwrap(),
                    )
                    .await
            } else {
                service
                    .create_billing_portal_session(
                        serde_json::from_value(
                            json!({"customerId":"cus_owned","expiresAt":"2026-01-01T00:00:00Z"}),
                        )
                        .unwrap(),
                    )
                    .await
            };
            assert_eq!(
                result.unwrap_err().message,
                "Invalid Stripe billing response"
            );
            assert_eq!(
                *state.calls.lock().await,
                ["POST /v1/billing_portal/sessions"]
            );
            state.calls.lock().await.clear();
            assert_eq!(
                serde_json::to_value(
                    service
                        .base()
                        .get_billing_portal_session("bps_old")
                        .await
                        .unwrap()
                )
                .unwrap(),
                old
            );
            assert!(
                service
                    .base()
                    .get_billing_portal_session("bps_new")
                    .await
                    .is_err()
            );
        }
    }
    verify_valid(&service, &state).await;
    server.abort();
}

async fn verify_valid(service: &StripeBillingService<MemoryBillingStore>, state: &Provider) {
    for (index, url) in [
        "https://example.test/new?secret=kept",
        "custom:verbatim",
        " relative-preserved ",
    ]
    .iter()
    .enumerate()
    {
        *state.result.lock().await =
            json!({"id":format!("bps_new_{index}"),"object":"billing_portal.session","url":url});
        let created = service
            .create_billing_portal_session(
                serde_json::from_value(
                    json!({"customerId":"cus_owned","expiresAt":"2026-01-01T00:00:00Z"}),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.uri.as_deref(), Some(*url));
        let updated = service
            .update_billing_portal_session(serde_json::from_value(json!({"id":"bps_old"})).unwrap())
            .await
            .unwrap();
        assert_eq!(updated.uri.as_deref(), Some(*url));
        assert_eq!(
            updated.provider_fields.as_ref().unwrap()["id"],
            format!("bps_new_{index}")
        );
        assert_eq!(
            *state.calls.lock().await,
            [
                "POST /v1/billing_portal/sessions",
                "POST /v1/billing_portal/sessions"
            ]
        );
        state.calls.lock().await.clear();
    }
}
