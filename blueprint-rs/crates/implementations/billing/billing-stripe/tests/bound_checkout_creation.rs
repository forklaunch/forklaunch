#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_checkout_fixture.rs"]
mod fixture;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{
        CheckoutCreationClaim, CheckoutCreationStatus, PostgresBillingStore,
        TrustedCheckoutPublisher,
    },
};
use forklaunch_billing_stripe::{
    bound_checkout_creation::{
        BoundCheckoutCreateRequest, BoundCheckoutCreationOutcome as Outcome,
        BoundCheckoutCreationProfile,
    },
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
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
    response: Arc<Mutex<Option<Value>>>,
    price: Arc<Mutex<Option<Value>>>,
    items: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer owned-checkout-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
fn price() -> Value {
    json!({"id":"custom-plan","object":"price","livemode":false,"active":true,"type":"recurring","billing_scheme":"per_unit","unit_amount":1200,"currency":"usd","recurring":{"interval":"month","interval_count":1,"usage_type":"licensed"},"product":"custom-product"})
}
fn plan() -> Value {
    json!({"id":"custom-plan","object":"plan","livemode":false,"active":true,"amount":1200,"currency":"usd","interval":"month","interval_count":1,"billing_scheme":"per_unit","usage_type":"licensed","product":{"id":"custom-product","object":"product","livemode":false,"active":true}})
}
fn session(n: usize) -> Value {
    let now = chrono::Utc::now().timestamp();
    json!({"id":format!("cs_owned_{n}"),"object":"checkout.session","livemode":false,"customer":"cus_owned","mode":"subscription","ui_mode":"hosted_page","status":"open","payment_status":"unpaid","payment_method_types":["card"],"success_url":"https://example.test/success","cancel_url":"https://example.test/cancel","currency":"usd","amount_subtotal":2400,"amount_total":2400,"automatic_tax":{"enabled":false},"allow_promotion_codes":false,"subscription":null,"payment_intent":null,"setup_intent":null,"payment_link":null,"invoice":null,"recovered_from":null,"total_details":{"amount_discount":0,"amount_shipping":0,"amount_tax":0},"created":now,"expires_at":now+3600,"url":"https://checkout.stripe.com/c/pay/owned-private"})
}
fn items(id: &str) -> Value {
    json!({"object":"list","has_more":false,"url":format!("/v1/checkout/sessions/{id}/line_items"),"data":[{"id":"li_owned","object":"item","quantity":2,"currency":"usd","amount_subtotal":2400,"amount_total":2400,"amount_discount":0,"amount_tax":0,"price":price()}]})
}
fn request(f: &Fixture) -> BoundCheckoutCreateRequest {
    BoundCheckoutCreateRequest {
        local_customer_id: f.customer,
        local_plan_id: f.plan,
        quantity: 2,
    }
}
async fn create(State(p): State<Provider>, h: HeaderMap, body: String) -> Json<Value> {
    auth(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-checkout-")
    );
    let fields: std::collections::BTreeMap<String, String> =
        reqwest::Url::parse(&format!("http://fixture/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(a, b)| (a.into_owned(), b.into_owned()))
            .collect();
    assert_eq!(
        fields,
        [
            ("customer", "cus_owned"),
            ("mode", "subscription"),
            ("ui_mode", "hosted_page"),
            ("line_items[0][price]", "custom-plan"),
            ("line_items[0][quantity]", "2"),
            ("payment_method_types[0]", "card"),
            ("success_url", "https://example.test/success"),
            ("cancel_url", "https://example.test/cancel"),
            ("allow_promotion_codes", "false"),
            ("automatic_tax[enabled]", "false"),
            ("adaptive_pricing[enabled]", "false")
        ]
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect()
    );
    let n = p.posts.fetch_add(1, Ordering::SeqCst) + 1;
    if p.pause.load(Ordering::SeqCst) == 1 {
        p.entered.notify_one();
        p.resume.notified().await;
    }
    Json(
        p.response
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| session(n)),
    )
}
#[tokio::test]
#[ignore = "owned isolated PostgreSQL roles/database and loopback provider only"]
async fn bound_checkout_uuid_dispatch_and_publication() {
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
            "/v1/customers/cus_owned",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"id":"cus_owned","object":"customer","livemode":false}))
            }),
        )
        .route(
            "/v1/plans/custom-plan",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(plan())
            }),
        )
        .route(
            "/v1/prices/custom-plan",
            get(|State(p): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                Json(p.price.lock().unwrap().clone().unwrap_or_else(price))
            }),
        )
        .route("/v1/checkout/sessions", post(create))
        .route(
            "/v1/checkout/sessions/{id}",
            get(
                |State(p): State<Provider>, Path(id): Path<String>, h: HeaderMap| async move {
                    auth(&h);
                    if p.pause.load(Ordering::SeqCst) == 2 {
                        p.entered.notify_one();
                        p.resume.notified().await;
                    }
                    let mut value = p
                        .response
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(|| session(1));
                    value["id"] = json!(id);
                    Json(value)
                },
            ),
        )
        .route(
            "/v1/checkout/sessions/{id}/line_items",
            get(
                |State(p): State<Provider>,
                 Path(id): Path<String>,
                 Query(q): Query<std::collections::BTreeMap<String, String>>,
                 h: HeaderMap| async move {
                    auth(&h);
                    assert_eq!(q.get("limit").map(String::as_str), Some("2"));
                    Json(
                        p.items
                            .lock()
                            .unwrap()
                            .clone()
                            .unwrap_or_else(|| items(&id)),
                    )
                },
            ),
        )
        .with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = StripeBillingClient::with_transport(
        "owned-checkout-key".into(),
        ProviderHttpClient::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            Duration::from_secs(7),
            1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_checkout(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher_pool = PgPoolOptions::new()
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
        .trusted_checkout_publisher(publisher_pool.clone())
        .await
        .unwrap();
    let profile = BoundCheckoutCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation)],
        "https://example.test/success".into(),
        "https://example.test/cancel".into(),
    )
    .unwrap();
    let service = StripeBillingService::new(BaseBillingService::new(f.store.clone()), client);
    for (field, bad) in [
        ("id", json!("wrong")),
        ("livemode", json!(true)),
        ("unit_amount", json!(1201)),
        ("product", json!("wrong")),
    ] {
        let mut value = price();
        value[field] = bad;
        *p.price.lock().unwrap() = Some(value);
        assert!(
            service
                .create_bound_checkout_session(&profile, &publisher, Uuid::new_v4(), request(f))
                .await
                .is_err()
        );
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    *p.price.lock().unwrap() = None;
    let op = Uuid::new_v4();
    let Outcome::Created { record, .. } = service
        .create_bound_checkout_session(&profile, &publisher, op, request(f))
        .await
        .unwrap()
    else {
        panic!("created")
    };
    assert!(Uuid::parse_str(&record.id).is_ok());
    assert_eq!(record.customer_id, f.customer.to_string());
    assert!(record.expires_at > chrono::Utc::now() + chrono::Duration::minutes(50));
    assert!(matches!(
        service
            .create_bound_checkout_session(&profile, &publisher, op, request(f))
            .await
            .unwrap(),
        Outcome::PreviouslyCompleted { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    assert_eq!(f.count("checkout_dependency_claims").await, 0);
    authority(f, op).await;
    Box::pin(read_created(f, &p, &service, &record.id)).await;
    // Pending claims prevent current/OLD-tenant relation changes during provider I/O.
    p.pause.store(1, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let call = service.create_bound_checkout_session(&profile, &publisher, op, request(f));
    tokio::pin!(call);
    tokio::select! {()=p.entered.notified()=>{},result=&mut call=>panic!("must pause {}",result.is_ok())}
    for sql in [
        "UPDATE fl_billing.records SET data=data WHERE kind IN ('customer','plan')",
        "UPDATE fl_billing.records SET tenant_id='other' WHERE kind='customer'",
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "DELETE FROM fl_billing.provider_bindings WHERE kind='plan'",
        "TRUNCATE fl_billing.checkout_creation_intents CASCADE",
        "DELETE FROM fl_billing.checkout_dependency_claims",
    ] {
        f.denied(sql, "checkout-a").await;
    }
    assert!(matches!(
        service
            .create_bound_checkout_session(&profile, &publisher, op, request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), 2);
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    assert!(matches!(call.await.unwrap(), Outcome::Created { .. }));
    // Wrong post-effect evidence never publishes; repeat returns pending without POST.
    for (field, bad) in [
        ("id", Value::Null),
        ("object", json!("customer")),
        ("customer", json!("cus_other")),
        ("livemode", json!(true)),
        ("status", json!("complete")),
        ("payment_status", json!("paid")),
        ("payment_link", json!("plink_x")),
        ("subscription", json!("sub_x")),
        ("amount_total", json!(2401)),
        ("expires_at", json!(0)),
        ("url", json!("\u{feff}")),
        ("ui_mode", json!("elements")),
    ] {
        let mut value = session(900);
        value[field] = bad;
        *p.response.lock().unwrap() = Some(value);
        let op = Uuid::new_v4();
        let before = p.posts.load(Ordering::SeqCst);
        let count = f.count("records").await;
        assert!(matches!(
            service
                .create_bound_checkout_session(&profile, &publisher, op, request(f))
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(p.posts.load(Ordering::SeqCst), before + 1);
        assert_eq!(f.count("records").await, count);
        assert!(matches!(
            service
                .create_bound_checkout_session(&profile, &publisher, op, request(f))
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(p.posts.load(Ordering::SeqCst), before + 1);
    }
    *p.response.lock().unwrap() = None;
    for field in ["has_more", "data", "url"] {
        let mut value = items("unused");
        value[field] = Value::Null;
        *p.items.lock().unwrap() = Some(value);
        let count = f.count("records").await;
        assert!(matches!(
            service
                .create_bound_checkout_session(&profile, &publisher, Uuid::new_v4(), request(f))
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.count("records").await, count);
    }
    *p.items.lock().unwrap() = None;
    faults(f, &p, &service, &profile, &publisher).await;
    cross_journal(f, &publisher_pool).await;
    lost_ack(f, &publisher_pool).await;
    server.abort();
    publisher_pool.close().await;
}
async fn store(pool: PgPool) -> PostgresBillingStore {
    PostgresBillingStore::new(
        pool,
        TenantContext::new("checkout-a").unwrap(),
        Arc::new(FieldEncryptor::new("owned-checkout-foundation-key").unwrap()),
        AuditContext::actor("service:checkout", "checkout-foundation"),
    )
    .await
    .unwrap()
}
#[allow(clippy::too_many_lines)] // One sequential rollback/unknown-ack transaction matrix.
async fn lost_ack(f: &Fixture, publisher_pool: &PgPool) {
    let publisher = f
        .store
        .trusted_checkout_publisher(publisher_pool.clone())
        .await
        .unwrap();
    let snap = f
        .store
        .snapshot_checkout_dependencies(
            &f.scope,
            f.customer,
            f.plan,
            f.plan_operation,
            STRIPE_API_VERSION,
        )
        .await
        .unwrap();
    let op = Uuid::new_v4();
    let CheckoutCreationClaim::Reserved(ticket) = f
        .store
        .reserve_checkout_creation(snap, op, &json!({"owned":"late-freshness"}), || Ok(()))
        .await
        .unwrap()
    else {
        panic!("reservation")
    };
    let record = json!({"id":ticket.public_id().to_string(),"customerId":f.customer.to_string(),"status":"PENDING","paymentMethods":["card"],"currency":"USD","uri":"https://checkout.stripe.com/owned-private","expiresAt":"2026-10-14T23:00:00Z","providerFields":session(1001)});
    let before = f.count("records").await;
    let claims = f.count("checkout_dependency_claims").await;
    let checks = AtomicUsize::new(0);
    assert!(
        publisher
            .complete(ticket, record, || {
                if checks.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(BlueprintError::new("expired", "Expired"))
                }
            })
            .await
            .is_err()
    );
    assert_eq!(f.count("records").await, before);
    assert_eq!(f.count("checkout_dependency_claims").await, claims);
    assert!(matches!(
        f.store.inspect_bound_checkout_creation(op).await.unwrap(),
        Some(CheckoutCreationStatus::Pending(_))
    ));
    drop(publisher);
    let options = f.pool.connect_options();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let proxy_options = (*options)
        .clone()
        .host("127.0.0.1")
        .port(port)
        .ssl_mode(sqlx::postgres::PgSslMode::Disable);
    let runtime = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(proxy_options.clone())
        .await
        .unwrap();
    let public_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(proxy_options.username(publisher_pool.connect_options().get_username()))
        .await
        .unwrap();
    let s = store(runtime.clone()).await;
    let publisher = s
        .trusted_checkout_publisher(public_pool.clone())
        .await
        .unwrap();
    let snap = s
        .snapshot_checkout_dependencies(
            &f.scope,
            f.customer,
            f.plan,
            f.plan_operation,
            STRIPE_API_VERSION,
        )
        .await
        .unwrap();
    let op = Uuid::new_v4();
    armed.store(true, Ordering::SeqCst);
    assert!(
        s.reserve_checkout_creation(snap, op, &json!({"owned":"reserve-ack"}), || Ok(()))
            .await
            .is_err()
    );
    assert!(s.inspect_checkout_creation(op).await.unwrap().is_some());
    let snap = s
        .snapshot_checkout_dependencies(
            &f.scope,
            f.customer,
            f.plan,
            f.plan_operation,
            STRIPE_API_VERSION,
        )
        .await
        .unwrap();
    let op = Uuid::new_v4();
    let CheckoutCreationClaim::Reserved(ticket) = s
        .reserve_checkout_creation(snap, op, &json!({"owned":"publish-ack"}), || Ok(()))
        .await
        .unwrap()
    else {
        panic!("reservation")
    };
    let record = json!({"id":ticket.public_id().to_string(),"customerId":f.customer.to_string(),"status":"PENDING","paymentMethods":["card"],"currency":"USD","uri":"https://checkout.stripe.com/owned-private","expiresAt":"2026-10-14T23:00:00Z","providerFields":session(1000)});
    armed.store(true, Ordering::SeqCst);
    assert!(publisher.complete(ticket, record, || Ok(())).await.is_err());
    assert!(matches!(
        s.inspect_bound_checkout_creation(op).await.unwrap(),
        Some(CheckoutCreationStatus::Completed { .. })
    ));
    drop(publisher);
    runtime.close().await;
    public_pool.close().await;
    proxy.abort();
}

// A valid Subscription completion releases its own four claims only; the shared
// Customer counter must still protect outstanding Checkout operations.
async fn cross_journal(f: &Fixture, publisher_pool: &PgPool) {
    use forklaunch_billing_base::services::postgres::PortalCreationClaim;
    let before = f.count("checkout_dependency_claims").await;
    assert!(before > 0);
    sqlx::query(sqlx::AssertSqlSafe(format!("GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher = f
        .store
        .trusted_portal_publisher(publisher_pool.clone())
        .await
        .unwrap();
    let snapshot = f
        .store
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let PortalCreationClaim::Reserved(ticket) = f
        .store
        .reserve_portal_creation(
            snapshot,
            Uuid::new_v4(),
            &json!({"owned":"cross-journal"}),
            || Ok(()),
        )
        .await
        .unwrap()
    else {
        panic!("new intent")
    };
    let record = json!({"id":ticket.public_id().to_string(),"customerId":f.customer.to_string(),"uri":"https://billing.stripe.com/owned-private","expiresAt":"2026-10-15T23:00:00Z","providerFields":{"id":"bps_owned_cross","object":"billing_portal.session"}});
    publisher.complete(ticket, record, || Ok(())).await.unwrap();
    assert_eq!(f.count("portal_dependency_claims").await, 0);
    assert_eq!(f.count("checkout_dependency_claims").await, before);
    f.denied(
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "checkout-a",
    )
    .await;
}
async fn faults(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundCheckoutCreationProfile,
    publisher: &TrustedCheckoutPublisher,
) {
    let count = f.count("records").await;
    sqlx::raw_sql("CREATE FUNCTION fl_billing.fail_checkout_audit_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='complete' THEN RAISE EXCEPTION 'owned audit fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_checkout_audit_fixture BEFORE INSERT ON fl_billing.checkout_creation_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.fail_checkout_audit_fixture()").execute(&f.owner).await.unwrap();
    assert!(matches!(
        service
            .create_bound_checkout_session(profile, publisher, Uuid::new_v4(), request(f))
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.count("records").await, count);
    sqlx::raw_sql("DROP TRIGGER fail_checkout_audit_fixture ON fl_billing.checkout_creation_audit; DROP FUNCTION fl_billing.fail_checkout_audit_fixture()").execute(&f.owner).await.unwrap();
    p.pause.store(1, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let call = service.create_bound_checkout_session(profile, publisher, op, request(f));
    tokio::pin!(call);
    tokio::select! {()=p.entered.notified()=>{},result=&mut call=>panic!("must pause {}",result.is_ok())}
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','checkout-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fl_billing.records WHERE kind='customer' FOR UPDATE")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5100)).await;
        tx.rollback().await.unwrap();
    });
    assert!(matches!(call.await.unwrap(), Outcome::Unresolved { .. }));
    release.await.unwrap();
    assert_eq!(f.count("records").await, count);
}

async fn authority(f: &Fixture, operation: Uuid) {
    for (tenant, actor) in [
        ("checkout-a", "service:other"),
        ("foreign-tenant", "service:checkout"),
    ] {
        let foreign = PostgresBillingStore::new(
            f.pool.clone(),
            TenantContext::new(tenant).unwrap(),
            Arc::new(FieldEncryptor::new("owned-checkout-foundation-key").unwrap()),
            AuditContext::actor(actor, "checkout-foundation"),
        )
        .await
        .unwrap();
        let result = foreign.inspect_bound_checkout_creation(operation).await;
        if tenant == "checkout-a" {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_none());
        }
    }
    let error=sqlx::query("SELECT fl_billing.publish_bound_checkout('checkout-a',$1::uuid,$1::uuid,'x','{}','{}','x','x','x')").bind(operation.to_string()).execute(&f.pool).await.unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("42501")
    );
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','checkout-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let changed=sqlx::query("UPDATE fl_billing.checkout_creation_intents SET state='completed',receipt_ciphertext='forged' WHERE operation_id=$1::uuid").bind(operation.to_string()).execute(&mut *tx).await.unwrap();
    assert_eq!(changed.rows_affected(), 0);
    tx.rollback().await.unwrap();
}

#[allow(clippy::too_many_lines)] // Keep the stateful read/tamper/race matrix on one owned fixture.
async fn read_created(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    id: &str,
) {
    let public = Uuid::parse_str(id).unwrap();
    let posts = p.posts.load(Ordering::SeqCst);
    let value = service
        .get_bound_created_checkout_session(&f.scope, public)
        .await
        .unwrap();
    assert_eq!(value.id, id);
    assert_eq!(value.customer_id, f.customer.to_string());
    assert_eq!(
        value.provider_fields.as_ref().unwrap()["customer"],
        "cus_owned"
    );
    for customer in [
        Value::Null,
        json!("cus_other"),
        json!({"id":"cus_owned","object":"customer","deleted":true}),
    ] {
        let mut value = session(1);
        value["customer"] = customer;
        *p.response.lock().unwrap() = Some(value);
        assert!(
            service
                .get_bound_created_checkout_session(&f.scope, public)
                .await
                .is_err()
        );
    }
    *p.response.lock().unwrap() = None;
    let binding = f
        .store
        .stripe_binding_by_public_id(
            &f.scope,
            forklaunch_billing_base::services::BillingKind::CheckoutSession,
            public,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .checkout_creation_read_snapshot(&binding, "wrong-version")
            .await
            .is_err()
    );
    for (tenant, actor) in [
        ("checkout-a", "service:other"),
        ("foreign-tenant", "service:checkout"),
    ] {
        let store = PostgresBillingStore::new(
            f.pool.clone(),
            TenantContext::new(tenant).unwrap(),
            Arc::new(FieldEncryptor::new("owned-checkout-foundation-key").unwrap()),
            AuditContext::actor(actor, "checkout-read"),
        )
        .await
        .unwrap();
        assert!(
            store
                .checkout_creation_read_snapshot(&binding, STRIPE_API_VERSION)
                .await
                .is_err()
        );
    }
    // Tampering with the public Customer field cannot downgrade the new getter
    // into the old administrative-alias profile, even with the correct cus_* ID.
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','checkout-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let original: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT data FROM fl_billing.records WHERE kind='checkout_session' AND id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{customerId}',to_jsonb('cus_owned'::text)) WHERE kind='checkout_session' AND id=$1").bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        service
            .get_bound_created_checkout_session(&f.scope, public)
            .await
            .is_err()
    );
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','checkout-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE fl_billing.records SET data=$1 WHERE kind='checkout_session' AND id=$2")
        .bind(original)
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        service
            .get_bound_created_checkout_session(&f.scope, public)
            .await
            .is_ok()
    );
    // Final audit insertion waits after provider I/O; fresh context must be
    // checked after that DB wait, not only before beginning the transaction.
    p.pause.store(2, Ordering::SeqCst);
    let deadline_call = service.get_bound_created_checkout_session(&f.scope, public);
    tokio::pin!(deadline_call);
    tokio::select! {()=p.entered.notified()=>{},result=&mut deadline_call=>panic!("must pause {}",result.is_ok())}
    let mut blocker = f.owner.begin().await.unwrap();
    sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5100)).await;
        blocker.rollback().await.unwrap();
    });
    assert!(deadline_call.await.is_err());
    release.await.unwrap();
    assert!(
        service
            .get_bound_created_checkout_session(&f.scope, public)
            .await
            .is_ok()
    );
    // A committed no-op body write changes xmin during the network read; old
    // creation evidence cannot authorize even an otherwise identical Customer.
    p.pause.store(2, Ordering::SeqCst);
    let call = service.get_bound_created_checkout_session(&f.scope, public);
    tokio::pin!(call);
    tokio::select! {()=p.entered.notified()=>{},result=&mut call=>panic!("must pause {}",result.is_ok())}
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','checkout-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE fl_billing.records SET data=data WHERE kind='customer'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    assert!(call.await.is_err());
    assert!(
        service
            .get_bound_created_checkout_session(&f.scope, public)
            .await
            .is_err()
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), posts);
}
