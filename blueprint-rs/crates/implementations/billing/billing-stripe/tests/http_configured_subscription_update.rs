#![cfg(feature = "http")]
#[path = "support/bound_portal_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use axum::{
    Json, Router,
    extract::State,
    http::{Method, Uri},
};
use fixture::Fixture;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_billing_interface::services::SubscriptionService;
use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_billing_stripe::http::{BillingBackend, BillingHttp};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use jsonwebtoken::{Algorithm, Validation};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;
const KEY: &[u8] = b"billing-owner-fixture";
fn sign(method: &str, path: &str, body: Option<&Value>, actor: &str) -> String {
    let encoded = body.map(Value::to_string);
    generate_hmac_auth_header(
        KEY,
        actor,
        method,
        path,
        encoded
            .as_deref()
            .map_or(SignedBody::Absent, SignedBody::Text),
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        &Uuid::new_v4().to_string(),
    )
    .unwrap()
}

async fn provider(
    State(calls): State<Arc<Mutex<Vec<String>>>>,
    method: Method,
    uri: Uri,
) -> Json<Value> {
    calls.lock().await.push(format!("{method} {}", uri.path()));
    Json(json!({"id":"sub_owned","object":"subscription","items":{"data":[]}}))
}
#[tokio::test]
#[ignore = "owned restricted PostgreSQL and actual backend HTTP"]
async fn configured_subscription_owner_backend() {
    let f = Arc::new(Fixture::new().await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let pcalls = calls.clone();
    let provider_task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().fallback(provider).with_state(pcalls),
        )
        .await
        .unwrap();
    });
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 4096).unwrap(),
    )
    .unwrap();
    let base = BaseBillingService::new(f.store.clone());
    let id = Uuid::new_v4().to_string();
    base.create_subscription(serde_json::from_value(json!({"id":id,"partyId":"local-party","partyType":"user","productId":"local-plan","active":true,"startDate":"2026-01-01T00:00:00Z","status":"ACTIVE","externalId":"sub_owned","billingProvider":"stripe"})).unwrap()).await.unwrap();
    let before = base.get_subscription(&id).await.unwrap();
    let backend = BillingBackend::stripe((*f.store).clone(), client.clone())
        .with_configured_subscription_updates(true);
    let auth = HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    let router = BillingHttp::new(backend, auth, ServiceInfo::new("billing", "1")).router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let http = reqwest::Client::new();
    let path = format!("/subscription/{id}");
    for correlation in [None, Some("owner-correlation"), Some("")] {
        for body in [
            json!({"partyId":"other"}),
            json!({"partyType":"organization"}),
            json!({"partyId":null}),
            json!({"partyType":null}),
            json!({"billingProvider":"invalid"}),
            json!({"partyType":"invalid"}),
        ] {
            let mut request = http
                .put(format!("{url}{path}"))
                .header("authorization", sign("PUT", &path, Some(&body), "default"))
                .json(&body);
            if let Some(value) = correlation {
                request = request.header("x-correlation-id", value);
            }
            let response = request.send().await.unwrap();
            let mapper_error = body["partyId"] == "other" || body["partyType"] == "organization";
            assert_eq!(response.status(), if mapper_error { 500 } else { 400 });
            let selected = response
                .headers()
                .get("x-correlation-id")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            if let Some(value) = correlation.filter(|value| !value.is_empty()) {
                assert_eq!(selected, value);
            } else {
                assert_eq!(Uuid::parse_str(&selected).unwrap().get_version_num(), 4);
            }
            if mapper_error {
                assert_eq!(
                    response.headers()["content-type"],
                    "text/plain; charset=utf-8"
                );
            }
            let error_body = response.text().await.unwrap();
            if mapper_error {
                assert_eq!(
                    error_body,
                    format!("Internal server error:\n\nCorrelation id: {selected}")
                );
                assert!(!error_body.contains("Subscription ownership"));
            }
            assert!(calls.lock().await.is_empty());
            assert_eq!(base.get_subscription(&id).await.unwrap(), before);
        }
    }
    for body in [
        json!({}),
        json!({"partyId":"local-party","partyType":"user"}),
    ] {
        let response = http
            .put(format!("{url}{path}"))
            .header("authorization", sign("PUT", &path, Some(&body), "default"))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let result = response.text().await.unwrap();
        assert_eq!(status, 200, "{result}");
        assert_eq!(
            *calls.lock().await,
            [
                "GET /v1/subscriptions/sub_owned",
                "POST /v1/subscriptions/sub_owned"
            ]
        );
        calls.lock().await.clear();
    }
    let missing = format!("/subscription/{}", Uuid::new_v4());
    let body = json!({"partyId":"other"});
    let response = http
        .put(format!("{url}{missing}"))
        .header(
            "authorization",
            sign("PUT", &missing, Some(&body), "default"),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    assert!(calls.lock().await.is_empty());
    server.abort();
    let auth = HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    let legacy = BillingHttp::new(
        BillingBackend::stripe((*f.store).clone(), client),
        auth,
        ServiceInfo::new("billing", "1"),
    )
    .router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let legacy_url = format!("http://{}", listener.local_addr().unwrap());
    let legacy_server = tokio::spawn(async move {
        axum::serve(listener, legacy.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let body = json!({"partyId":"generic-other"});
    let response = http
        .put(format!("{legacy_url}{path}"))
        .header("authorization", sign("PUT", &path, Some(&body), "default"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!response.headers().contains_key("x-correlation-id"));
    assert_eq!(
        base.get_subscription(&id).await.unwrap().party_id,
        "generic-other"
    );
    assert_eq!(
        *calls.lock().await,
        [
            "GET /v1/subscriptions/sub_owned",
            "POST /v1/subscriptions/sub_owned"
        ]
    );
    legacy_server.abort();
    provider_task.abort();
}
