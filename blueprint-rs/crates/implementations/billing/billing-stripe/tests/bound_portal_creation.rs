#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_portal_fixture.rs"]
mod fixture;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{
        PortalCreationClaim, PortalCreationStatus, PostgresBillingStore, TrustedPortalPublisher,
    },
};
use forklaunch_billing_stripe::{
    bound_portal_creation::{BoundPortalCreationOutcome as Outcome, BoundPortalCreationProfile},
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
#[derive(Default, Clone)]
struct Provider {
    posts: Arc<AtomicUsize>,
    response: Arc<Mutex<Option<Value>>>,
    config: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer owned-portal-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
fn session(n: usize) -> Value {
    json!({"id":format!("bps_owned_{n}"),"object":"billing_portal.session","livemode":false,"customer":"cus_owned","configuration":"bpc_owned","created":1_791_936_000,"flow":null,"on_behalf_of":null,"return_url":"https://example.test/account","url":"https://billing.stripe.com/p/session?secret=owned-private"})
}
async fn create(State(p): State<Provider>, h: HeaderMap, body: String) -> Json<Value> {
    auth(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-portal-")
    );
    let fields: std::collections::BTreeMap<String, String> =
        reqwest::Url::parse(&format!("http://fixture/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    assert_eq!(
        fields,
        [
            ("customer".into(), "cus_owned".into()),
            ("configuration".into(), "bpc_owned".into()),
            ("return_url".into(), "https://example.test/account".into())
        ]
        .into()
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
#[ignore = "requires owned isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn bound_portal_uuid_dispatch_barriers_and_publication() {
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
async fn matrix(f: &Fixture) {
    let p = Provider::default();
    let app=Router::new().route("/v1/account",get(|h:HeaderMap|async move{auth(&h);Json(json!({"id":"acct_owned","object":"account"}))}))
 .route("/v1/balance",get(|h:HeaderMap|async move{auth(&h);Json(json!({"object":"balance","livemode":false}))}))
 .route("/v1/customers/cus_owned",get(|h:HeaderMap|async move{auth(&h);Json(json!({"id":"cus_owned","object":"customer","livemode":false}))}))
 .route("/v1/billing_portal/configurations/bpc_owned",get(|State(p):State<Provider>,h:HeaderMap|async move{auth(&h);assert!(!h.contains_key("idempotency-key"));Json(p.config.lock().unwrap().clone().unwrap_or_else(||json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":false,"active":true})))}))
 .route("/v1/billing_portal/sessions",post(create)).with_state(p.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = StripeBillingClient::with_transport(
        "owned-portal-key".into(),
        ProviderHttpClient::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            Duration::from_secs(7),
            1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    assert!(
        f.store
            .trusted_portal_publisher(f.pool.clone())
            .await
            .is_err()
    );
    let publisher = f
        .store
        .trusted_portal_publisher(pool.clone())
        .await
        .unwrap();
    let service = StripeBillingService::new(BaseBillingService::new(f.store.clone()), client);
    let profile = BoundPortalCreationProfile::trusted(
        f.scope.clone(),
        "bpc_owned".into(),
        "https://example.test/account".into(),
    )
    .unwrap();
    Box::pin(cases(f, &p, &service, &profile, &publisher)).await;
    cross_journal(f, &pool).await;
    lost_ack(f, &pool).await;
    drop(publisher);
    pool.close().await;
    server.abort();
}
#[allow(clippy::too_many_lines)]
async fn cases(
    f: &Fixture,
    p: &Provider,
    service: &StripeBillingService<PostgresBillingStore>,
    profile: &BoundPortalCreationProfile,
    publisher: &TrustedPortalPublisher,
) {
    for bad in [
        json!({"id":"other","object":"billing_portal.configuration","livemode":false,"active":true}),
        json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":false,"active":false}),
        json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":true,"active":true}),
    ] {
        *p.config.lock().unwrap() = Some(bad);
        assert!(
            service
                .create_bound_portal_session(profile, publisher, Uuid::new_v4(), f.customer)
                .await
                .is_err()
        );
    }
    *p.config.lock().unwrap() = None;
    assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    assert_eq!(f.count("portal_creation_intents").await, 0);
    let op = Uuid::new_v4();
    let result = service
        .create_bound_portal_session(profile, publisher, op, f.customer)
        .await
        .unwrap();
    let public = match result {
        Outcome::Created { record, .. } => {
            assert_ne!(record.id, "bps_owned_1");
            assert_eq!(record.customer_id, f.customer.to_string());
            assert!(record.uri.unwrap().contains("owned-private"));
            Uuid::parse_str(&record.id).unwrap()
        }
        _ => panic!("new portal expected"),
    };
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    assert_eq!(f.count("portal_dependency_claims").await, 0);
    assert!(
        matches!(f.store.inspect_bound_portal_creation(op).await.unwrap(),Some(PortalCreationStatus::Completed{public_id,..}) if public_id==public)
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(
        matches!(service.create_bound_portal_session(profile,publisher,op,f.customer).await.unwrap(),Outcome::PreviouslyCompleted{public_id,..} if public_id==public)
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let changed = BoundPortalCreationProfile::trusted(
        f.scope.clone(),
        "bpc_owned".into(),
        "https://example.test/changed".into(),
    )
    .unwrap();
    assert!(
        service
            .create_bound_portal_session(&changed, publisher, op, f.customer)
            .await
            .is_err()
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let stored: String =
        sqlx::query_scalar("SELECT data::text FROM fl_billing.records WHERE kind='billing_portal'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(!stored.contains("owned-private"));
    assert_eq!(sqlx::query("UPDATE fl_billing.portal_creation_intents SET state='pending' WHERE operation_id=$1::uuid").bind(op.to_string()).execute(&mut *tx).await.unwrap().rows_affected(),0);
    tx.rollback().await.unwrap();
    // Historical inspection is distinct from a fresh creation request. Changed
    // remote config or revoked/replaced Customer authority denies create replay.
    *p.config.lock().unwrap() = Some(
        json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":false,"active":false}),
    );
    assert!(
        service
            .create_bound_portal_session(profile, publisher, op, f.customer)
            .await
            .is_err()
    );
    *p.config.lock().unwrap() = None;
    let binding = f
        .store
        .stripe_customer_binding(&f.scope, f.customer)
        .await
        .unwrap()
        .unwrap();
    f.store
        .revoke_stripe_customer_binding(&binding)
        .await
        .unwrap();
    assert!(
        service
            .create_bound_portal_session(profile, publisher, op, f.customer)
            .await
            .is_err()
    );
    assert!(matches!(
        f.store.inspect_bound_portal_creation(op).await.unwrap(),
        Some(PortalCreationStatus::Completed { .. })
    ));
    let association =
        forklaunch_billing_base::services::postgres::TrustedCustomerParty::administrative(
            f.customer,
            "user".into(),
            "external-party-assertion".into(),
        )
        .unwrap();
    let snap = f
        .store
        .snapshot_customer_registration(&f.scope, &association, "cus_owned", STRIPE_API_VERSION)
        .await
        .unwrap();
    f.store
        .complete_customer_registration(snap, || Ok(()))
        .await
        .unwrap();
    assert!(
        service
            .create_bound_portal_session(profile, publisher, op, f.customer)
            .await
            .is_err()
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    // A held provider request retains both immutable dependencies; duplicate callers never POST.
    p.pause.store(1, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let call = service.create_bound_portal_session(profile, publisher, op, f.customer);
    tokio::pin!(call);
    tokio::select! {()=p.entered.notified()=>{},result=&mut call=>panic!("must pause: {}",result.is_ok())}
    for sql in [
        "UPDATE fl_billing.records SET data=data WHERE kind='customer'",
        "UPDATE fl_billing.records SET tenant_id='other' WHERE kind='customer'",
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "TRUNCATE fl_billing.records CASCADE",
        "DELETE FROM fl_billing.portal_dependency_claims",
        "TRUNCATE fl_billing.portal_creation_intents CASCADE",
    ] {
        f.denied(sql, "portal-a").await;
    }
    assert!(matches!(
        service
            .create_bound_portal_session(profile, publisher, op, f.customer)
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(p.posts.load(Ordering::SeqCst), 2);
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    assert!(matches!(call.await.unwrap(), Outcome::Created { .. }));
    assert_eq!(f.count("portal_dependency_claims").await, 0);
    // Ordinary runtime cannot invoke the trusted publisher, despite broad table grants.
    let denied=sqlx::query("SELECT fl_billing.publish_bound_portal('portal-a',$1::uuid,$1::uuid,'x','{}','{}','x','x','x')").bind(Uuid::new_v4().to_string()).execute(&f.pool).await.unwrap_err();
    assert_eq!(
        denied
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("42501")
    );
    // Complete-audit failure rolls back publication/release after the provider POST.
    sqlx::raw_sql("CREATE FUNCTION fl_billing.fail_portal_audit_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='complete' THEN RAISE EXCEPTION 'owned audit fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_portal_audit_fixture BEFORE INSERT ON fl_billing.portal_creation_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.fail_portal_audit_fixture()").execute(&f.owner).await.unwrap();
    let before = f.count("records").await;
    assert!(matches!(
        service
            .create_bound_portal_session(profile, publisher, Uuid::new_v4(), f.customer)
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.count("records").await, before);
    assert_eq!(f.count("portal_dependency_claims").await, 2);
    sqlx::raw_sql("DROP TRIGGER fail_portal_audit_fixture ON fl_billing.portal_creation_audit; DROP FUNCTION fl_billing.fail_portal_audit_fixture()").execute(&f.owner).await.unwrap();
    // A publisher blocked on the Customer row must recheck freshness after the wait.
    p.pause.store(1, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let delayed = service.create_bound_portal_session(profile, publisher, op, f.customer);
    tokio::pin!(delayed);
    tokio::select! {()=p.entered.notified()=>{},result=&mut delayed=>panic!("must pause: {}",result.is_ok())}
    let mut lock = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fl_billing.records WHERE kind='customer' FOR UPDATE")
        .fetch_all(&mut *lock)
        .await
        .unwrap();
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    let unlock = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5100)).await;
        lock.rollback().await.unwrap();
    });
    assert!(matches!(delayed.await.unwrap(), Outcome::Unresolved { .. }));
    unlock.await.unwrap();
    assert_eq!(f.count("records").await, before);
    assert_eq!(f.count("portal_dependency_claims").await, 4);
    // Malformed provider response has already consumed one POST; no local record/retry/release.
    for (field, value) in [
        ("id", Value::Null),
        ("id", json!("")),
        ("object", json!("customer")),
        ("customer", json!("cus_other")),
        ("configuration", json!("bpc_other")),
        ("livemode", json!(true)),
        ("flow", json!({})),
        ("on_behalf_of", json!("acct_other")),
        ("customer_account", json!("acct_other")),
        ("url", Value::Null),
        ("url", json!("\u{feff}")),
        ("return_url", json!("https://other.test")),
        ("created", json!(false)),
    ] {
        let mut bad = session(999);
        bad[field] = value;
        *p.response.lock().unwrap() = Some(bad);
        let before = p.posts.load(Ordering::SeqCst);
        let records = f.count("records").await;
        let op = Uuid::new_v4();
        assert!(matches!(
            service
                .create_bound_portal_session(profile, publisher, op, f.customer)
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(p.posts.load(Ordering::SeqCst), before + 1);
        assert_eq!(f.count("records").await, records);
        assert!(matches!(
            service
                .create_bound_portal_session(profile, publisher, op, f.customer)
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(p.posts.load(Ordering::SeqCst), before + 1);
    }
    *p.response.lock().unwrap() = None;
    // Shared counters must retain every unresolved Portal claim; Subscription release is unrelated.
    assert_eq!(f.count("portal_dependency_claims").await, 30);
    f.denied(
        "UPDATE fl_billing.records SET data=data WHERE kind='customer'",
        "portal-a",
    )
    .await;
}
async fn store(pool: PgPool) -> PostgresBillingStore {
    PostgresBillingStore::new(
        pool,
        TenantContext::new("portal-a").unwrap(),
        Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
        AuditContext::actor("service:portal", "portal-foundation"),
    )
    .await
    .unwrap()
}
#[allow(clippy::too_many_lines)] // One sequential rollback/unknown-ack transaction matrix.
async fn lost_ack(f: &Fixture, publisher_pool: &PgPool) {
    let publisher = f
        .store
        .trusted_portal_publisher(publisher_pool.clone())
        .await
        .unwrap();
    let snap = f
        .store
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let op = Uuid::new_v4();
    let PortalCreationClaim::Reserved(ticket) = f
        .store
        .reserve_portal_creation(snap, op, &json!({"owned":"late-freshness"}), || Ok(()))
        .await
        .unwrap()
    else {
        panic!("reservation")
    };
    let record = json!({"id":ticket.public_id().to_string(),"customerId":f.customer.to_string(),"uri":"https://billing.stripe.com/owned-private","expiresAt":"2026-10-14T23:00:00Z","providerFields":session(1001)});
    let before = f.count("records").await;
    let claims = f.count("portal_dependency_claims").await;
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
    assert_eq!(f.count("portal_dependency_claims").await, claims);
    assert!(matches!(
        f.store.inspect_bound_portal_creation(op).await.unwrap(),
        Some(PortalCreationStatus::Pending(_))
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
        .trusted_portal_publisher(public_pool.clone())
        .await
        .unwrap();
    let snap = s
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let op = Uuid::new_v4();
    armed.store(true, Ordering::SeqCst);
    assert!(
        s.reserve_portal_creation(snap, op, &json!({"owned":"reserve-ack"}), || Ok(()))
            .await
            .is_err()
    );
    assert!(s.inspect_portal_creation(op).await.unwrap().is_some());
    let snap = s
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let op = Uuid::new_v4();
    let PortalCreationClaim::Reserved(ticket) = s
        .reserve_portal_creation(snap, op, &json!({"owned":"publish-ack"}), || Ok(()))
        .await
        .unwrap()
    else {
        panic!("reservation")
    };
    let record = json!({"id":ticket.public_id().to_string(),"customerId":f.customer.to_string(),"uri":"https://billing.stripe.com/owned-private","expiresAt":"2026-10-14T23:00:00Z","providerFields":session(1000)});
    armed.store(true, Ordering::SeqCst);
    assert!(publisher.complete(ticket, record, || Ok(())).await.is_err());
    assert!(matches!(
        s.inspect_bound_portal_creation(op).await.unwrap(),
        Some(PortalCreationStatus::Completed { .. })
    ));
    drop(publisher);
    runtime.close().await;
    public_pool.close().await;
    proxy.abort();
}

// A valid Subscription completion releases its own four claims only; the shared
// Customer counter must still protect outstanding Portal operations.
async fn cross_journal(f: &Fixture, pool: &PgPool) {
    use forklaunch_billing_base::services::postgres::{
        PlanCreationClaim, SubscriptionCreationClaim,
    };
    let plan_operation = Uuid::new_v4();
    let PlanCreationClaim::Reserved(ticket)=f.store.reserve_plan_creation(&f.scope,plan_operation,STRIPE_API_VERSION,&json!({"request":{"name":"Pro","active":true,"currency":"USD","cadence":"month","unitAmount":1200,"productId":"custom-product"}})).await.unwrap() else {panic!("plan")};
    let plan = ticket.public_id();
    let product = json!({"id":"custom-product","object":"product","livemode":false});
    let record = json!({"id":plan.to_string(),"name":"Pro","price":1200,"active":true,"currency":"USD","cadence":"month","externalId":"custom-plan","billingProvider":"stripe","providerFields":product,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"});
    f.store
        .complete_plan_creation(
            ticket,
            record,
            json!({"id":"custom-plan","object":"plan","livemode":false,"product":product}),
            || Ok(()),
        )
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher = f
        .store
        .trusted_subscription_publisher(pool.clone())
        .await
        .unwrap();
    let snapshot = f
        .store
        .snapshot_subscription_dependencies(
            &f.scope,
            f.customer,
            plan,
            plan_operation,
            STRIPE_API_VERSION,
        )
        .await
        .unwrap();
    let SubscriptionCreationClaim::Reserved(ticket) = f
        .store
        .reserve_subscription_creation(
            snapshot,
            Uuid::new_v4(),
            &json!({"owned":"cross-journal"}),
            || Ok(()),
        )
        .await
        .unwrap()
    else {
        panic!("subscription")
    };
    let before = f.count("portal_dependency_claims").await;
    let record = json!({"id":ticket.public_id().to_string(),"partyId":"external-party-assertion","partyType":"user","productId":plan.to_string(),"externalId":"sub_cross_journal","billingProvider":"stripe","status":"incomplete","active":false,"startDate":"2026-01-01T00:00:00Z","providerFields":{"id":"sub_cross_journal","object":"subscription"}});
    publisher.complete(ticket, record, || Ok(())).await.unwrap();
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    assert_eq!(f.count("portal_dependency_claims").await, before);
    f.denied(
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "portal-a",
    )
    .await;
}
