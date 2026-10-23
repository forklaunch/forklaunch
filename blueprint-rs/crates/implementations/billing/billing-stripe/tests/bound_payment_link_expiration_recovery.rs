#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_portal_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::get,
};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{
        PaymentLinkCreationClaim, PostgresBillingStore, TrustedPaymentLinkExpirationPublisher,
    },
};
use forklaunch_billing_stripe::{
    bound_payment_link_expiration::{
        BoundPaymentLinkExpirationOutcome, BoundPaymentLinkExpirationProfile,
    },
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgPoolOptions, PgSslMode},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;
#[derive(Clone, Default)]
struct Wire {
    posts: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    invalid: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicBool>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
    arm: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    event: Arc<Mutex<Value>>,
    current: Arc<Mutex<Option<Value>>>,
    slow: Arc<AtomicBool>,
    key: Arc<Mutex<String>>,
}
fn headers(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer expiration-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
async fn post(
    State(w): State<Wire>,
    Path(id): Path<String>,
    h: HeaderMap,
    body: String,
) -> Json<Value> {
    headers(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-plink-expire-")
    );
    assert_eq!(body, "metadata%5Bstatus%5D=EXPIRED");
    *w.key.lock().unwrap() = h["idempotency-key"].to_str().unwrap().into();
    w.posts.fetch_add(1, Ordering::SeqCst);
    if w.pause.load(Ordering::SeqCst) {
        w.entered.notify_one();
        w.resume.notified().await;
    }
    if let Some(arm) = w.arm.lock().unwrap().as_ref() {
        arm.store(true, Ordering::SeqCst);
    }
    Json(w.invalid.lock().unwrap().clone().unwrap_or_else(||json!({"id":id,"object":"payment_link","livemode":false,"active":true,"metadata":{"status":"EXPIRED"}})))
}
async fn server(w: Wire) -> (StripeBillingClient, tokio::task::JoinHandle<()>) {
    let app = Router::new().route("/v1/events/{id}",get(|Path(id):Path<String>,State(w):State<Wire>,h:HeaderMap|async move {headers(&h);assert_eq!(id,"evt_owned");Json(w.event.lock().unwrap().clone())}))
        .route(
            "/v1/account",
            get(|h: HeaderMap| async move {
                headers(&h);
                Json(json!({"id":"acct_owned","object":"account"}))
            }),
        )
        .route(
            "/v1/balance",
            get(|h: HeaderMap| async move {
                headers(&h);
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route(
            "/v1/payment_links/{id}",
            get(|Path(id): Path<String>,State(w):State<Wire>, h: HeaderMap| async move {
                if w.slow.load(Ordering::SeqCst){tokio::time::sleep(Duration::from_millis(5100)).await;}
                if let Some(arm)=w.arm.lock().unwrap().as_ref(){arm.store(true,Ordering::SeqCst);}
                headers(&h);
                Json(w.current.lock().unwrap().clone().unwrap_or_else(||json!({"id":id,"object":"payment_link","livemode":false,"active":true,"metadata":{"status":"EXPIRED"}})))
            })
            .post(post),
        )
        .with_state(w.clone())
        .layer(axum::middleware::from_fn(
            move |r: axum::extract::Request, n: axum::middleware::Next| {
                let w = w.clone();
                async move {
                    w.calls.fetch_add(1, Ordering::SeqCst);
                    n.run(r).await
                }
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        StripeBillingClient::with_transport(
            "expiration-key".into(),
            ProviderHttpClient::new(&url, Duration::from_secs(8), 1024 * 1024).unwrap(),
        )
        .unwrap(),
        task,
    )
}
fn profile(f: &Fixture) -> BoundPaymentLinkExpirationProfile {
    BoundPaymentLinkExpirationProfile::trusted(f.scope.clone(), vec!["price_owned".into()]).unwrap()
}
async fn seed(f: &Fixture) -> Uuid {
    let operation = Uuid::new_v4();
    let request = json!({"amount":1200,"currency":"USD","paymentMethods":["card"],"status":"PENDING","providerFields":{"line_items":[{"price":"price_owned","quantity":1}]}});
    let payload = json!({"request":request,"providerFields":{"line_items":[{"price":"price_owned","quantity":1}],"currency":"USD","payment_method_types":["card"]},"trustedPrices":["price_owned"]});
    let PaymentLinkCreationClaim::Reserved(ticket) = f
        .store
        .reserve_payment_link_creation(&f.scope, operation, STRIPE_API_VERSION, &payload)
        .await
        .unwrap()
    else {
        panic!("reserved")
    };
    let id = ticket.public_id();
    let record = json!({"id":id.to_string(),"amount":1200,"currency":"USD","paymentMethods":["card"],"status":"PENDING","providerFields":{"id":format!("plink_{}",id.simple()),"object":"payment_link","livemode":false},"createdAt":"2026-10-15T00:00:00Z","updatedAt":"2026-10-15T00:00:00Z"});
    f.store
        .complete_payment_link_creation(ticket, record, || Ok(()))
        .await
        .unwrap();
    id
}
async fn publisher(f: &Fixture, pool: PgPool) -> TrustedPaymentLinkExpirationPublisher {
    f.store
        .trusted_payment_link_expiration_publisher(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "owned isolated PostgreSQL, authenticated GET evidence and separate publisher"]
async fn event_linked_expiration_recovery() {
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
fn event(w: &Wire, id: Uuid) -> Value {
    json!({"id":"evt_owned","object":"event","type":"payment_link.updated","created":chrono::Utc::now().timestamp(),"api_version":STRIPE_API_VERSION,"livemode":false,"request":{"id":"req_owned","idempotency_key":w.key.lock().unwrap().clone()},"data":{"object":{"id":format!("plink_{}",id.simple()),"object":"payment_link","livemode":false,"active":true,"metadata":{"status":"EXPIRED"}}}})
}
async fn pending(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) -> (Uuid, Uuid) {
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    *w.invalid.lock().unwrap() = Some(json!({"object":"ambiguous"}));
    assert!(matches!(
        service
            .expire_bound_created_payment_link(publisher, &profile(f), operation, id)
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    *w.event.lock().unwrap() = event(w, id);
    (id, operation)
}
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {0}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {0}; GRANT EXECUTE ON FUNCTION fl_billing.publish_payment_link_expiration(text,uuid,uuid,text,jsonb,text,text) TO {0}",f.publisher_role))).execute(&f.owner).await.unwrap();
    let pp = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let publisher = publisher(f, pp.clone()).await;
    let w = Wire::default();
    let (client, task) = server(w.clone()).await;
    let service =
        StripeBillingService::new(BaseBillingService::new(f.store.clone()), client.clone());
    let (id, operation) = pending(f, &service, &publisher, &w).await;
    let calls = w.calls.load(Ordering::SeqCst);
    assert!(
        service
            .recover_bound_payment_link_expiration(
                &publisher,
                &profile(f),
                operation,
                id,
                "bad/event"
            )
            .await
            .is_err()
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE SELECT ON fl_billing.payment_link_expiration_dependency_claims FROM {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        service
            .recover_bound_payment_link_expiration(
                &publisher,
                &profile(f),
                operation,
                id,
                "evt_owned"
            )
            .await
            .is_err()
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT SELECT ON fl_billing.payment_link_expiration_dependency_claims TO {role}; REVOKE INSERT ON fl_billing.payment_link_expiration_dependency_claims,fl_billing.payment_link_expiration_intents FROM {role}"))).execute(&f.owner).await.unwrap();
    let baseline = w.event.lock().unwrap().clone();
    let posts = w.posts.load(Ordering::SeqCst);
    for (pointer, value) in [
        ("/request", Value::Null),
        ("/request/idempotency_key", Value::Null),
        ("/request/idempotency_key", json!("metadata-only-no-key")),
        ("/api_version", json!("old")),
        ("/livemode", json!(true)),
        ("/type", json!("payment_link.created")),
        ("/data/object/metadata/status", json!("ACTIVE")),
        ("/data/object/id", json!("plink_other")),
        ("/created", json!(0)),
        ("/created", json!(chrono::Utc::now().timestamp() + 301)),
        ("/created", json!(chrono::Utc::now().timestamp() + 60)),
    ] {
        let mut bad = baseline.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        *w.event.lock().unwrap() = bad;
        assert!(
            service
                .recover_bound_payment_link_expiration(
                    &publisher,
                    &profile(f),
                    operation,
                    id,
                    "evt_owned"
                )
                .await
                .is_err(),
            "{pointer}"
        );
        assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    }
    for field in ["account", "context"] {
        let mut bad = baseline.clone();
        bad[field] = json!("other");
        *w.event.lock().unwrap() = bad;
        assert!(
            service
                .recover_bound_payment_link_expiration(
                    &publisher,
                    &profile(f),
                    operation,
                    id,
                    "evt_owned"
                )
                .await
                .is_err()
        );
    }
    *w.event.lock().unwrap() = baseline.clone();
    let current = baseline["data"]["object"].clone();
    for (field, value) in [
        ("id", json!("plink_other")),
        ("object", json!("price")),
        ("livemode", json!(true)),
        ("metadata", Value::Null),
    ] {
        let mut bad = current.clone();
        bad[field] = value;
        *w.current.lock().unwrap() = Some(bad);
        assert!(
            service
                .recover_bound_payment_link_expiration(
                    &publisher,
                    &profile(f),
                    operation,
                    id,
                    "evt_owned"
                )
                .await
                .is_err()
        );
        assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    }
    // active=false is not a completion prerequisite or a state transition inferred here.
    let mut inactive = current;
    inactive["active"] = json!(false);
    *w.current.lock().unwrap() = Some(inactive);
    let changed =
        BoundPaymentLinkExpirationProfile::trusted(f.scope.clone(), vec!["price_other".into()])
            .unwrap();
    let calls = w.calls.load(Ordering::SeqCst);
    assert!(
        service
            .recover_bound_payment_link_expiration(&publisher, &changed, operation, id, "evt_owned")
            .await
            .is_err()
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    let foreign = Arc::new(
        f.store
            .with_audit_context(AuditContext::actor("foreign", "recovery"))
            .unwrap(),
    );
    let foreign_publisher = publisher
        .with_audit_context(AuditContext::actor("foreign", "recovery"))
        .unwrap();
    let other = StripeBillingService::new(BaseBillingService::new(foreign), client.clone());
    assert!(
        other
            .recover_bound_payment_link_expiration(
                &foreign_publisher,
                &profile(f),
                operation,
                id,
                "evt_owned"
            )
            .await
            .is_err()
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    sqlx::raw_sql("CREATE FUNCTION fl_billing.owned_recovery_audit_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='complete' THEN RAISE EXCEPTION 'owned audit fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER owned_recovery_audit_failure BEFORE INSERT ON fl_billing.payment_link_expiration_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.owned_recovery_audit_failure()").execute(&f.owner).await.unwrap();
    assert!(matches!(
        service
            .recover_bound_payment_link_expiration(
                &publisher,
                &profile(f),
                operation,
                id,
                "evt_owned"
            )
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        2
    );
    sqlx::raw_sql(
        "DROP TRIGGER owned_recovery_audit_failure ON fl_billing.payment_link_expiration_audit",
    )
    .execute(&f.owner)
    .await
    .unwrap();
    // Evidence admission rejects hostile nesting before cloning/canonicalization or release.
    let mut nested = Value::Null;
    for _ in 0..40 {
        nested = json!([nested]);
    }
    let mut deep = baseline.clone();
    deep["extra"] = nested;
    *w.event.lock().unwrap() = deep;
    assert!(matches!(
        service
            .recover_bound_payment_link_expiration(
                &publisher,
                &profile(f),
                operation,
                id,
                "evt_owned"
            )
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        2
    );
    *w.event.lock().unwrap() = baseline;
    let p = profile(f);
    let (first, second) = tokio::join!(
        service.recover_bound_payment_link_expiration(&publisher, &p, operation, id, "evt_owned"),
        service.recover_bound_payment_link_expiration(&publisher, &p, operation, id, "evt_owned")
    );
    assert!(matches!(
        first.unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
    assert!(matches!(
        second.unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
    assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        0
    );
    let calls = w.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .recover_bound_payment_link_expiration(&publisher, &p, operation, id, "evt_owned")
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let encrypted:String=sqlx::query_scalar("SELECT receipt_ciphertext FROM fl_billing.payment_link_expiration_intents WHERE operation_id=$1::uuid").bind(operation.to_string()).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let receipt: Value = serde_json::from_str(
        &FieldEncryptor::new("owned-portal-foundation-key")
            .unwrap()
            .decrypt(&encrypted, "portal-a")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt["value"]["recovery"]["kind"],
        "event_linked_metadata_expiration_v1"
    );
    assert_eq!(
        receipt["value"]["recovery"]["evidence"]["event"]["request"]["idempotency_key"],
        *w.key.lock().unwrap()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT INSERT ON fl_billing.payment_link_expiration_dependency_claims,fl_billing.payment_link_expiration_intents TO {role}"))).execute(&f.owner).await.unwrap();
    *w.current.lock().unwrap() = None;
    // Drop a real final COMMIT acknowledgement: receipt inspection acknowledges the winner.
    let (id, operation) = pending(f, &service, &publisher, &w).await;
    w.slow.store(true, Ordering::SeqCst);
    let posts = w.posts.load(Ordering::SeqCst);
    assert!(
        service
            .recover_bound_payment_link_expiration(&publisher, &p, operation, id, "evt_owned")
            .await
            .is_err()
    );
    assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        2
    );
    w.slow.store(false, Ordering::SeqCst);
    let options = (*pp.connect_options()).clone();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let proxy_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            options
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    let rp = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    let store = Arc::new(
        PostgresBillingStore::new(
            rp.clone(),
            TenantContext::new("portal-a").unwrap(),
            Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
            AuditContext::actor("service:portal", "recovery-proxy"),
        )
        .await
        .unwrap(),
    );
    let proxy_publisher = store
        .trusted_payment_link_expiration_publisher(proxy_pool.clone())
        .await
        .unwrap();
    let proxied = StripeBillingService::new(BaseBillingService::new(store), client.clone());
    *w.arm.lock().unwrap() = Some(armed.clone());
    let posts = w.posts.load(Ordering::SeqCst);
    assert!(matches!(
        proxied
            .recover_bound_payment_link_expiration(&proxy_publisher, &p, operation, id, "evt_owned")
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        0
    );
    *w.arm.lock().unwrap() = None;
    proxy_pool.close().await;
    rp.close().await;
    proxy.abort();
    task.abort();
    pp.close().await;
}
