use super::fixture::Fixture;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_billing_stripe::{
    bound_subscription_creation::BoundSubscriptionCreateRequest,
    client::{STRIPE_API_VERSION, StripeBillingClient},
};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
#[derive(Clone, Default)]
pub struct Provider {
    pub posts: Arc<AtomicUsize>,
    pub requests: Arc<AtomicUsize>,
    customer_response: Arc<Mutex<Option<Value>>>,
    plan_response: Arc<Mutex<Option<Value>>>,
    pub event: Arc<Mutex<Value>>,
    event_reads: Arc<AtomicUsize>,
    response: Arc<Mutex<Option<Value>>>,
    pub readback: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicUsize>,
    delay_ms: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer owned-subscription-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
fn plan() -> Value {
    json!({"id":"custom-plan","object":"plan","livemode":false,"active":true,"amount":1200,"currency":"usd","interval":"month","interval_count":1,"usage_type":"licensed","billing_scheme":"per_unit","product":{"id":"custom-product","object":"product","livemode":false}})
}
pub fn subscription(n: usize) -> Value {
    let id = format!("sub_wire{n}");
    let price = json!({"id":"custom-plan","object":"price","livemode":false,"active":true,"type":"recurring","unit_amount":1200,"currency":"usd","billing_scheme":"per_unit","recurring":{"interval":"month","interval_count":1,"usage_type":"licensed"},"product":"custom-product"});
    json!({"id":id,"object":"subscription","livemode":false,"customer":"cus_owned","status":"incomplete","collection_method":"charge_automatically","start_date":1_791_936_000,"items":{"object":"list","has_more":false,"data":[{"id":format!("si_wire{n}"),"object":"subscription_item","subscription":id,"quantity":2,"plan":plan(),"price":price}]}})
}
async fn paused(p: &Provider, stage: usize) {
    if p.pause.load(Ordering::SeqCst) == stage {
        p.entered.notify_one();
        p.resume.notified().await;
    }
}
async fn create(State(p): State<Provider>, h: HeaderMap, body: String) -> Json<Value> {
    auth(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-subscription-")
    );
    let form: std::collections::BTreeMap<_, _> =
        reqwest::Url::parse(&format!("http://fixture/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    let expected: std::collections::BTreeMap<String, String> = [
        ("customer", "cus_owned"),
        ("items[0][plan]", "custom-plan"),
        ("items[0][quantity]", "2"),
        ("collection_method", "charge_automatically"),
        ("payment_behavior", "default_incomplete"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    assert_eq!(form, expected);
    let n = p.posts.fetch_add(1, Ordering::SeqCst) + 1;
    paused(&p, 1).await;
    tokio::time::sleep(Duration::from_millis(
        u64::try_from(p.delay_ms.load(Ordering::SeqCst)).unwrap(),
    ))
    .await;
    Json(
        p.response
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| subscription(n)),
    )
}
async fn read(State(p): State<Provider>, h: HeaderMap) -> Json<Value> {
    auth(&h);
    assert!(!h.contains_key("idempotency-key"));
    paused(&p, 2).await;
    Json(
        p.readback
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| subscription(p.posts.load(Ordering::SeqCst))),
    )
}
pub fn request(f: &Fixture) -> BoundSubscriptionCreateRequest {
    BoundSubscriptionCreateRequest {
        local_customer_id: f.customer,
        local_plan_id: f.plan,
        quantity: 2,
        description: None,
    }
}
pub async fn start() -> (Provider, StripeBillingClient, tokio::task::JoinHandle<()>) {
    let p = Provider::default();
    let app = Router::new()
        .route(
            "/v1/account",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"id":"acct_owned","object":"account"}))
            }),
        )
        .route(
            "/v1/balance",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route(
            "/v1/customers/{id}",
            get(|State(p): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                Json(p.customer_response.lock().unwrap().clone().unwrap_or_else(
                    || json!({"id":"cus_owned","object":"customer","livemode":false}),
                ))
            }),
        )
        .route(
            "/v1/plans/{id}",
            get(|State(p): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                Json(p.plan_response.lock().unwrap().clone().unwrap_or_else(plan))
            }),
        )
        .route(
            "/v1/events/{id}",
            get(|State(p): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                assert!(!h.contains_key("idempotency-key"));
                p.event_reads.fetch_add(1, Ordering::SeqCst);
                Json(p.event.lock().unwrap().clone())
            }),
        )
        .route("/v1/subscriptions", post(create))
        .route("/v1/subscriptions/{id}", get(read))
        .with_state(p.clone())
        .layer(axum::middleware::from_fn({
            let count = p.requests.clone();
            move |r: axum::extract::Request, n: axum::middleware::Next| {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    n.run(r).await
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = StripeBillingClient::with_transport(
        "owned-subscription-key".into(),
        ProviderHttpClient::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            Duration::from_secs(2),
            1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (p, client, server)
}
