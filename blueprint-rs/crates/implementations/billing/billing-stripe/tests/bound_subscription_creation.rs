#![cfg(feature = "postgres-identity")]
#[path = "support/bound_subscription_fixture.rs"]
mod fixture;
#[path = "support/bound_subscription_read_cases.rs"]
mod read_cases;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService, postgres::TrustedSubscriptionPublisher,
};
use forklaunch_billing_stripe::{
    bound_subscription_creation::{
        BoundSubscriptionCreateRequest, BoundSubscriptionCreationOutcome as Outcome,
        BoundSubscriptionCreationProfile, SubscriptionCollectionPolicy,
    },
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone, Default)]
struct Provider {
    posts: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    customer_response: Arc<Mutex<Option<Value>>>,
    plan_response: Arc<Mutex<Option<Value>>>,
    event: Arc<Mutex<Value>>,
    event_reads: Arc<AtomicUsize>,
    response: Arc<Mutex<Option<Value>>>,
    readback: Arc<Mutex<Option<Value>>>,
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
fn subscription(n: usize) -> Value {
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
fn request(f: &Fixture) -> BoundSubscriptionCreateRequest {
    BoundSubscriptionCreateRequest {
        local_customer_id: f.customer,
        local_plan_id: f.plan,
        quantity: 2,
        description: None,
    }
}
#[tokio::test]
#[ignore = "requires owned isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn bound_subscription_native_wire_matrix() {
    let f = Arc::new(Fixture::new().await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run, false)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
// Keep connection/loopback teardown with the assertions even when a case panics.
async fn matrix(f: &Fixture, reads: bool) {
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
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let publisher = f
        .store
        .trusted_subscription_publisher(pool.clone())
        .await
        .unwrap();
    let service = StripeBillingService::new(BaseBillingService::new(f.store.clone()), client);
    let profile = BoundSubscriptionCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation)],
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .unwrap();
    if reads {
        read_cases::matrix(f, &p, &service, &profile, &publisher).await;
    } else {
        cases(f, &p, &service, &profile, &publisher).await;
        recovery_cases(f, &p, &service, &profile, &publisher).await;
    }
    drop(publisher);
    pool.close().await;
    server.abort();
}
#[allow(clippy::too_many_lines)]
async fn cases(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &BoundSubscriptionCreationProfile,
    publisher: &TrustedSubscriptionPublisher,
) {
    let mut invalid = request(f);
    invalid.quantity = 0;
    assert!(
        service
            .create_bound_subscription(profile, publisher, Uuid::new_v4(), invalid)
            .await
            .is_err()
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    assert_eq!(f.count("subscription_creation_intents").await, 0);
    let op = Uuid::new_v4();
    let Outcome::Completed { record, .. } = service
        .create_bound_subscription(profile, publisher, op, request(f))
        .await
        .unwrap()
    else {
        panic!("valid incomplete receipt")
    };
    assert_eq!(record.external_id, "sub_wire1");
    assert_eq!(record.status, "incomplete");
    assert!(!record.active);
    assert!(Uuid::parse_str(&record.id).is_ok());
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    assert!(matches!(
        service
            .create_bound_subscription(profile, publisher, op, request(f))
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    // Both POST and readback are inside durable claims. Raw record/binding
    // changes must fail under the correctly authorized tenant too.
    for stage in [1, 2] {
        p.pause.store(stage, Ordering::SeqCst);
        let operation = Uuid::new_v4();
        let create = service.create_bound_subscription(profile, publisher, operation, request(f));
        let barrier = async {
            p.entered.notified().await;
            let posts = p.posts.load(Ordering::SeqCst);
            assert!(matches!(
                service
                    .create_bound_subscription(profile, publisher, operation, request(f))
                    .await
                    .unwrap(),
                Outcome::Unresolved { .. }
            ));
            assert_eq!(p.posts.load(Ordering::SeqCst), posts);

            f.denied(
                "UPDATE fl_billing.records SET data=data WHERE kind IN ('customer','plan')",
                "subscription-a",
            )
            .await;
            f.denied("UPDATE fl_billing.provider_bindings SET binding_id=gen_random_uuid() WHERE kind='plan'","subscription-a").await;
            p.resume.notify_one();
        };
        let (result, ()) = tokio::join!(create, barrier);
        assert!(matches!(result.unwrap(), Outcome::Completed { .. }));
        p.pause.store(0, Ordering::SeqCst);
    }
    // Active is only the provider's service status. An unpaid/open invoice
    // stays untouched in encrypted evidence and is never projected as paid.
    let mut active = subscription(p.posts.load(Ordering::SeqCst) + 1);
    active["status"] = json!("active");
    active["latest_invoice"] =
        json!({"id":"in_unpaid","object":"invoice","paid":false,"status":"open"});
    *p.response.lock().unwrap() = Some(active.clone());
    *p.readback.lock().unwrap() = Some(active);
    let Outcome::Completed { record, .. } = service
        .create_bound_subscription(profile, publisher, Uuid::new_v4(), request(f))
        .await
        .unwrap()
    else {
        panic!("active service state")
    };
    assert!(record.active);
    assert_eq!(record.status, "active");
    assert_eq!(
        record.provider_fields.as_ref().unwrap()["latest_invoice"]["paid"],
        json!(false)
    );
    *p.response.lock().unwrap() = None;
    *p.readback.lock().unwrap() = None;
    let records = f.count("records").await;
    for (path, value) in [
        ("/status", json!("trialing")),
        ("/customer", json!("cus_other")),
        ("/livemode", json!(true)),
        ("/items/data/0/price/unit_amount", json!(1201)),
        ("/items/data/0/price/recurring/interval", json!("year")),
        ("/items/has_more", json!(true)),
        ("/items/data/0/quantity", json!(3)),
    ] {
        let n = p.posts.load(Ordering::SeqCst) + 1;
        let mut bad = subscription(n);
        *bad.pointer_mut(path).unwrap() = value;
        *p.response.lock().unwrap() = Some(bad);
        let op = Uuid::new_v4();
        assert!(
            matches!(
                service
                    .create_bound_subscription(profile, publisher, op, request(f))
                    .await
                    .unwrap(),
                Outcome::Unresolved { .. }
            ),
            "{path}"
        );
        assert_eq!(f.count("records").await, records);
        assert!(matches!(
            service
                .create_bound_subscription(profile, publisher, op, request(f))
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(p.posts.load(Ordering::SeqCst), n);
    }
    *p.response.lock().unwrap() = None;
    let mut wrong = subscription(p.posts.load(Ordering::SeqCst) + 1);
    wrong["id"] = json!("sub_wrong");
    *p.readback.lock().unwrap() = Some(wrong);
    assert!(matches!(
        service
            .create_bound_subscription(profile, publisher, Uuid::new_v4(), request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.count("records").await, records);
    *p.readback.lock().unwrap() = None;
    p.delay_ms.store(2100, Ordering::SeqCst);
    let timeout_op = Uuid::new_v4();
    let before = p.posts.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .create_bound_subscription(profile, publisher, timeout_op, request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(matches!(
        service
            .create_bound_subscription(profile, publisher, timeout_op, request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), before + 1);
    assert_eq!(f.count("records").await, records);
}

#[allow(clippy::too_many_lines)]
async fn recovery_cases(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &BoundSubscriptionCreationProfile,
    publisher: &TrustedSubscriptionPublisher,
) {
    p.delay_ms.store(0, Ordering::SeqCst);
    *p.readback.lock().unwrap() = None;
    let operation = Uuid::new_v4();
    *p.response.lock().unwrap() = Some(json!({"ambiguous":"owned bad response"}));
    assert!(matches!(
        service
            .create_bound_subscription(profile, publisher, operation, request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    *p.response.lock().unwrap() = None;
    let posts = p.posts.load(Ordering::SeqCst);
    let records = f.count("records").await;
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let (key,created):(String,i64)=sqlx::query_as("SELECT idempotency_key,floor(extract(epoch FROM created_at))::bigint FROM fl_billing.subscription_creation_intents WHERE operation_id=$1::uuid").bind(operation.to_string()).fetch_one(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let mut original = subscription(posts);
    original["created"] = json!(created);
    let event = json!({"id":"evt_owned","object":"event","type":"customer.subscription.created","api_version":STRIPE_API_VERSION,"livemode":false,"created":created,"request":{"id":"req_owned","idempotency_key":key},"data":{"object":original},"account":null,"context":null});
    let mut current = original.clone();
    current["status"] = json!("active");
    *p.readback.lock().unwrap() = Some(current.clone());
    for (path, bad) in [
        ("/id", json!("evt_other")),
        ("/object", json!("subscription")),
        ("/type", json!("customer.subscription.updated")),
        ("/request", json!(null)),
        ("/request/id", json!(null)),
        ("/request/idempotency_key", json!(null)),
        ("/request/idempotency_key", json!("different")),
        ("/api_version", json!("2019-02-19")),
        ("/api_version", json!(null)),
        ("/livemode", json!(true)),
        ("/account", json!("acct_other")),
        ("/context", json!("ctx_other")),
        ("/created", json!(created + 86_400)),
        ("/created", json!(created + 200)),
        ("/created", json!(created - 6)),
        ("/data/object/created", json!(created - 6)),
        ("/data/object/customer", json!("cus_other")),
        ("/data/object/items/data/0/quantity", json!(3)),
        ("/data/object/items/data/0/price/unit_amount", json!(1201)),
    ] {
        let mut bad_event = event.clone();
        *bad_event.pointer_mut(path).unwrap() = bad;
        bad_event["data"]["object"]["metadata"] =
            json!({"operationId":operation.to_string(),"idempotencyKey":key});
        *p.event.lock().unwrap() = bad_event;
        assert!(
            service
                .recover_bound_subscription(profile, publisher, operation, "evt_owned")
                .await
                .is_err(),
            "{path}"
        );
        assert_eq!(p.posts.load(Ordering::SeqCst), posts);
        assert_eq!(f.count("records").await, records);
    }
    *p.event.lock().unwrap() = event.clone();
    for (path, bad) in [
        ("/created", json!(created + 1)),
        ("/items/data/0/id", json!("si_other")),
        ("/items/data/0/price/currency", json!("eur")),
        ("/status", json!("canceled")),
    ] {
        let mut bad_current = current.clone();
        *bad_current.pointer_mut(path).unwrap() = bad;
        *p.readback.lock().unwrap() = Some(bad_current);
        assert!(
            service
                .recover_bound_subscription(profile, publisher, operation, "evt_owned")
                .await
                .is_err(),
            "{path}"
        );
    }
    let mut active_event = event.clone();
    active_event["data"]["object"]["status"] = json!("active");
    *p.event.lock().unwrap() = active_event;
    *p.readback.lock().unwrap() = Some(original);
    assert!(
        service
            .recover_bound_subscription(profile, publisher, operation, "evt_owned")
            .await
            .is_err()
    );
    assert_eq!(f.count("records").await, records);
    *p.event.lock().unwrap() = event.clone();
    *p.readback.lock().unwrap() = Some(current);
    let changed_profile = BoundSubscriptionCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation), (Uuid::new_v4(), Uuid::new_v4())],
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .unwrap();
    let before = p.event_reads.load(Ordering::SeqCst);
    assert!(
        service
            .recover_bound_subscription(&changed_profile, publisher, operation, "evt_owned")
            .await
            .is_err()
    );
    assert_eq!(p.event_reads.load(Ordering::SeqCst), before);
    let (a, b) = tokio::join!(
        service.recover_bound_subscription(profile, publisher, operation, "evt_owned"),
        service.recover_bound_subscription(profile, publisher, operation, "evt_owned")
    );
    assert!(
        matches!(a, Ok(Outcome::Completed { .. })) || matches!(b, Ok(Outcome::Completed { .. }))
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), posts);
    assert_eq!(f.count("records").await, records + 1);
    let evidence = f
        .store
        .inspect_subscription_recovery_evidence(operation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(evidence["event"], event);
    assert_eq!(evidence["eventHash"].as_str().unwrap().len(), 64);
    let Some(forklaunch_billing_base::services::postgres::SubscriptionCreationStatus::Completed {
        record: recovered,
        ..
    }) = f
        .store
        .inspect_bound_subscription_creation(operation)
        .await
        .unwrap()
    else {
        panic!("recovered receipt")
    };
    let recovered_id = Uuid::parse_str(recovered["id"].as_str().unwrap()).unwrap();
    assert!(
        service
            .get_bound_created_subscription(profile, recovered_id)
            .await
            .is_ok()
    );

    assert!(matches!(
        service
            .recover_bound_subscription(profile, publisher, operation, "evt_owned")
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), posts);
}

#[tokio::test]
#[ignore = "requires owned isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn bound_subscription_created_read_matrix() {
    let f = Arc::new(Fixture::new_with_plan_actor("service:catalog").await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run, true)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
