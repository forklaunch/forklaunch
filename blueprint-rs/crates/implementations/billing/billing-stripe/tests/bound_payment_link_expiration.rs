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
    BaseBillingService, BillingKind,
    postgres::{
        PaymentLinkCreationClaim, PaymentLinkExpirationClaim, PaymentLinkExpirationStatus,
        PostgresBillingStore, TrustedPaymentLinkExpirationPublisher,
    },
};
use forklaunch_billing_stripe::{
    bound_creation::BoundPaymentLinkCreationProfile,
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
    let app = Router::new()
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
            get(|Path(id): Path<String>, h: HeaderMap| async move {
                headers(&h);
                Json(json!({"id":id,"object":"payment_link","livemode":false,"active":true}))
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
async fn snapshot(
    f: &Fixture,
    id: Uuid,
) -> forklaunch_billing_base::services::postgres::PaymentLinkCreationReadSnapshot {
    let b = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::PaymentLink, id)
        .await
        .unwrap()
        .unwrap();
    f.store
        .payment_link_creation_read_snapshot(&b, STRIPE_API_VERSION)
        .await
        .unwrap()
}
async fn owner_sql(f: &Fixture, sql: &str) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(&mut *tx)
        .await;
    if result.is_ok() {
        tx.commit().await.unwrap();
    }
    result
}
#[tokio::test]
#[ignore = "owned isolated PostgreSQL, separate publisher and exact loopback wire"]
async fn expiration_single_dispatch_and_claims_matrix() {
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
    assert!(
        f.store
            .trusted_payment_link_expiration_publisher(f.pool.clone())
            .await
            .is_err()
    );
    let w = Wire::default();
    let (client, task) = server(w.clone()).await;
    let service = Arc::new(StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        client.clone(),
    ));
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    // Wrong actor fails before provider reads; publisher actor must match runtime too.
    let other = StripeBillingService::new(
        BaseBillingService::new(Arc::new(
            f.store
                .with_audit_context(AuditContext::actor("other", "expiration"))
                .unwrap(),
        )),
        client.clone(),
    );
    assert!(
        other
            .expire_bound_created_payment_link(&publisher, &profile(f), operation, id)
            .await
            .is_err()
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), 0);
    denied_scope_and_grants(f, &service, &publisher, &client, &w, id).await;
    let concurrent_profile = profile(f);
    let (a, b) = tokio::join!(
        service.expire_bound_created_payment_link(&publisher, &concurrent_profile, operation, id),
        service.expire_bound_created_payment_link(&publisher, &concurrent_profile, operation, id)
    );
    assert!(a.is_ok() && b.is_ok());
    assert_eq!(w.posts.load(Ordering::SeqCst), 1);
    assert!(matches!(
        service
            .inspect_bound_payment_link_expiration(&profile(f), operation, id)
            .await
            .unwrap(),
        Some(BoundPaymentLinkExpirationOutcome::Completed { .. })
    ));
    let calls = w.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .expire_bound_created_payment_link(&publisher, &profile(f), operation, id)
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
    assert_eq!(w.calls.load(Ordering::SeqCst), calls);
    let read_profile =
        BoundPaymentLinkCreationProfile::trusted(f.scope.clone(), vec!["price_owned".into()])
            .unwrap();
    assert!(
        service
            .get_bound_created_payment_link(&read_profile, id)
            .await
            .is_err()
    );
    assert_eq!(
        f.count("payment_link_expiration_dependency_claims").await,
        0
    );
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    // Mutable record restoration cannot erase the completed local tombstone.
    owner_sql(
        f,
        &format!("UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"),
    )
    .await
    .unwrap();
    assert!(
        service
            .get_bound_created_payment_link(&read_profile, id)
            .await
            .is_err()
    );
    let mut state_tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *state_tx)
        .await
        .unwrap();
    let status:String=sqlx::query_scalar("SELECT data->>'status' FROM fl_billing.records WHERE tenant_id='portal-a' AND kind='payment_link' AND id=$1").bind(id.to_string()).fetch_one(&mut *state_tx).await.unwrap();
    assert_eq!(status, "PENDING");
    state_tx.rollback().await.unwrap();
    malformed(f, &service, &publisher, &w).await;
    held_claims(f, &service, &publisher, &w).await;
    during_dispatch(f, &service, &publisher, &w).await;
    audit_rollback(f, &service, &publisher, &w).await;
    stale_and_failed_reservation(f).await;
    old_tenant_and_statement_race(f).await;
    stale_after_provider(f, &service, &publisher, &w).await;
    lost_ack(f, &service, &client, &pp, &w).await;
    task.abort();
    drop(publisher);
    pp.close().await;
}
async fn malformed(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) {
    for bad in [
        Value::Null,
        json!({"id":"wrong","object":"payment_link","livemode":false,"metadata":{"status":"EXPIRED"}}),
        json!({"id":"unchosen","object":"customer","livemode":false}),
        json!({"livemode":true}),
    ] {
        let id = seed(f).await;
        let operation = Uuid::new_v4();
        *w.invalid.lock().unwrap() = Some(bad);
        let before = w.posts.load(Ordering::SeqCst);
        assert!(matches!(
            service
                .expire_bound_created_payment_link(publisher, &profile(f), operation, id)
                .await
                .unwrap(),
            BoundPaymentLinkExpirationOutcome::Unresolved { .. }
        ));
        *w.invalid.lock().unwrap() = None;
        let restarted = StripeBillingService::new(
            BaseBillingService::new(f.store.clone()),
            service.client().clone(),
        );
        assert!(matches!(
            restarted
                .expire_bound_created_payment_link(publisher, &profile(f), operation, id)
                .await
                .unwrap(),
            BoundPaymentLinkExpirationOutcome::Unresolved { .. }
        ));
        assert_eq!(w.posts.load(Ordering::SeqCst), before + 1);
        assert!(
            service
                .expire_bound_created_payment_link(publisher, &profile(f), Uuid::new_v4(), id)
                .await
                .is_err()
        );
        assert_eq!(w.posts.load(Ordering::SeqCst), before + 1);
        assert!(owner_sql(f,&format!("UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'")).await.is_err());
    }
}
async fn held_claims(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) {
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    let snap = snapshot(f, id).await;
    let PaymentLinkExpirationClaim::Reserved(ticket) = f
        .store
        .reserve_payment_link_expiration(operation, snap, &json!(["price_owned"]), || Ok(()))
        .await
        .unwrap()
    else {
        panic!("reserved")
    };
    drop(ticket); // crash before dispatch: inspection never recreates a ticket.
    let before = w.posts.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .expire_bound_created_payment_link(publisher, &profile(f), operation, id)
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    assert_eq!(before, w.posts.load(Ordering::SeqCst));
    for sql in [
        format!(
            "UPDATE fl_billing.records SET tenant_id='other',id='changed' WHERE kind='payment_link' AND id='{id}'"
        ),
        format!("DELETE FROM fl_billing.provider_bindings WHERE public_id='{id}'"),
        "TRUNCATE fl_billing.records CASCADE".into(),
        "TRUNCATE fl_billing.payment_link_expiration_intents CASCADE".into(),
        "TRUNCATE fl_billing.payment_link_expiration_audit".into(),
        "DELETE FROM fl_billing.payment_link_expiration_dependency_claims".into(),
    ] {
        f.denied(&sql, "portal-a").await;
    }
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(sqlx::query("UPDATE fl_billing.payment_link_expiration_intents SET state='completed',receipt_ciphertext='forged' WHERE operation_id=$1::uuid").bind(operation.to_string()).execute(&mut *tx).await.unwrap().rows_affected(),0);
    tx.rollback().await.unwrap();
    assert!(sqlx::query("SELECT fl_billing.publish_payment_link_expiration('portal-a',$1::uuid,$1::uuid,'hash','{}','forged','test')").bind(operation.to_string()).execute(&f.pool).await.is_err());
}
async fn lost_ack(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    client: &StripeBillingClient,
    pp: &PgPool,
    w: &Wire,
) {
    let options = pp.connect_options();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*options)
                .clone()
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .unwrap();
    // Proxy changes configured port, so construct both explicit connections through it.
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
            AuditContext::actor("service:portal", "expiration-proxy"),
        )
        .await
        .unwrap(),
    );
    let publisher = store
        .trusted_payment_link_expiration_publisher(pool.clone())
        .await
        .unwrap();
    let proxied = StripeBillingService::new(BaseBillingService::new(store.clone()), client.clone());
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    *w.arm.lock().unwrap() = Some(armed.clone());
    assert!(matches!(
        proxied
            .expire_bound_created_payment_link(&publisher, &profile(f), operation, id)
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    *w.arm.lock().unwrap() = None;
    assert!(!armed.load(Ordering::SeqCst));
    assert!(matches!(
        service
            .inspect_bound_payment_link_expiration(&profile(f), operation, id)
            .await
            .unwrap(),
        Some(BoundPaymentLinkExpirationOutcome::Completed { .. })
    ));
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    let snap = snapshot(f, id).await;
    armed.store(true, Ordering::SeqCst);
    assert!(
        store
            .reserve_payment_link_expiration(operation, snap, &json!(["price_owned"]), || Ok(()))
            .await
            .is_err()
    );
    assert!(!armed.load(Ordering::SeqCst));
    assert!(matches!(
        f.store
            .inspect_payment_link_expiration(
                &f.scope,
                STRIPE_API_VERSION,
                operation,
                id,
                &json!(["price_owned"])
            )
            .await
            .unwrap(),
        Some(PaymentLinkExpirationStatus::Pending { .. })
    ));
    drop(publisher);
    pool.close().await;
    rp.close().await;
    proxy.abort();
}
async fn during_dispatch(
    f: &Fixture,
    service: &Arc<StripeBillingService<PostgresBillingStore>>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) {
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    let worker = service.clone();
    let owned_profile = profile(f);
    let publisher = publisher
        .with_audit_context(AuditContext::actor("service:portal", "during-dispatch"))
        .unwrap();
    w.pause.store(true, Ordering::SeqCst);
    let task = tokio::task::spawn_local(async move {
        worker
            .expire_bound_created_payment_link(&publisher, &owned_profile, operation, id)
            .await
    });
    w.entered.notified().await;
    assert!(owner_sql(f,&format!("UPDATE fl_billing.provider_bindings SET binding_id=gen_random_uuid() WHERE kind='payment_link' AND public_id='{id}'")).await.is_err());
    assert!(
        owner_sql(
            f,
            &format!(
                "UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"
            )
        )
        .await
        .is_err()
    );
    // Unrelated unclaimed rows remain mutable, including cached->durable promotion.
    let other = seed(f).await;
    owner_sql(
        f,
        &format!(
            "UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{other}'"
        ),
    )
    .await
    .unwrap();
    owner_sql(f,"INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data,expires_at) VALUES('portal-a','cache','payment_link','unclaimed-cached','{\"id\":\"unclaimed-cached\"}',now()+interval '1 day'); UPDATE fl_billing.records SET scope='durable',expires_at=NULL WHERE id='unclaimed-cached'; UPDATE fl_billing.records SET data=data WHERE id='unclaimed-cached'").await.unwrap();
    w.pause.store(false, Ordering::SeqCst);
    w.resume.notify_one();
    assert!(matches!(
        task.await.unwrap().unwrap(),
        BoundPaymentLinkExpirationOutcome::Completed { .. }
    ));
}
async fn audit_rollback(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) {
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    let before = w.posts.load(Ordering::SeqCst);
    sqlx::raw_sql("CREATE FUNCTION fl_billing.owned_expiration_audit_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='complete' THEN RAISE EXCEPTION 'owned audit fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER owned_expiration_audit_failure BEFORE INSERT ON fl_billing.payment_link_expiration_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.owned_expiration_audit_failure()").execute(&f.owner).await.unwrap();
    assert!(matches!(
        service
            .expire_bound_created_payment_link(publisher, &profile(f), operation, id)
            .await
            .unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    sqlx::raw_sql("DROP TRIGGER owned_expiration_audit_failure ON fl_billing.payment_link_expiration_audit; DROP FUNCTION fl_billing.owned_expiration_audit_failure()").execute(&f.owner).await.unwrap();
    assert_eq!(w.posts.load(Ordering::SeqCst), before + 1);
    assert!(matches!(
        f.store
            .inspect_payment_link_expiration(
                &f.scope,
                STRIPE_API_VERSION,
                operation,
                id,
                &json!(["price_owned"])
            )
            .await
            .unwrap(),
        Some(PaymentLinkExpirationStatus::Pending { .. })
    ));
    assert!(
        owner_sql(
            f,
            &format!(
                "UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"
            )
        )
        .await
        .is_err()
    );
}
async fn stale_and_failed_reservation(f: &Fixture) {
    let id = seed(f).await;
    let snap = snapshot(f, id).await;
    let before = f.count("payment_link_expiration_intents").await;
    owner_sql(
        f,
        &format!("UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"),
    )
    .await
    .unwrap();
    assert!(
        f.store
            .reserve_payment_link_expiration(Uuid::new_v4(), snap, &json!(["price_owned"]), || Ok(
                ()
            ))
            .await
            .is_err()
    );
    assert_eq!(f.count("payment_link_expiration_intents").await, before);
    let snap = snapshot(f, id).await;
    let checks = AtomicUsize::new(0);
    assert!(
        f.store
            .reserve_payment_link_expiration(Uuid::new_v4(), snap, &json!(["price_owned"]), || {
                if checks.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err(forklaunch_core::domain::BlueprintError::new(
                        "owned_expired",
                        "expired",
                    ))
                }
            })
            .await
            .is_err()
    );
    assert_eq!(f.count("payment_link_expiration_intents").await, before);
    // Claim rollback is immediately observable without sleep/retry.
    owner_sql(
        f,
        &format!("UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"),
    )
    .await
    .unwrap();
}
async fn old_tenant_and_statement_race(f: &Fixture) {
    let id = seed(f).await;
    let snap = snapshot(f, id).await;
    let mut locker = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *locker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fl_billing.records WHERE tenant_id='portal-a' AND kind='payment_link' AND id=$1 FOR UPDATE").bind(id.to_string()).execute(&mut *locker).await.unwrap();
    let store = f.store.clone();
    let reserve = tokio::task::spawn_local(async move {
        store
            .reserve_payment_link_expiration(Uuid::new_v4(), snap, &json!(["price_owned"]), || {
                Ok(())
            })
            .await
    });
    let role = f.pool.connect_options().get_username().to_owned();
    wait_blocked(f, &role, 1).await;
    let pool = f.pool.clone();
    let mutator = tokio::task::spawn_local(async move {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE fl_billing.records SET data=data WHERE tenant_id='portal-a' AND kind='payment_link' AND id=$1").bind(id.to_string()).execute(&mut *tx).await
    });
    wait_blocked(f, &role, 2).await;
    locker.commit().await.unwrap();
    assert!(matches!(
        reserve.await.unwrap().unwrap(),
        PaymentLinkExpirationClaim::Reserved(_)
    ));
    assert!(
        mutator.await.unwrap().is_err(),
        "statement started before reservation must observe locked current counter"
    );
    // Make the actual non-bypass migration owner able to address OLD rows with a
    // wrong tenant GUC. The SECURITY DEFINER barrier must still use OLD identity.
    let owner = f.owner.connect_options().get_username().to_owned();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE POLICY owned_expiration_probe ON fl_billing.records TO {owner} USING(true) WITH CHECK(true)"))).execute(&f.owner).await.unwrap();
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','wrong',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("UPDATE fl_billing.records SET tenant_id='changed',id='rebound' WHERE tenant_id='portal-a' AND id=$1").bind(id.to_string()).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::raw_sql("DROP POLICY owned_expiration_probe ON fl_billing.records")
        .execute(&f.owner)
        .await
        .unwrap();
}
async fn wait_blocked(f: &Fixture, role: &str, count: i64) {
    let end = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND usename=$1 AND cardinality(pg_blocking_pids(pid))>0").bind(role).fetch_one(&f.pool).await.unwrap();
        if waiting >= count {
            return;
        }
        assert!(
            std::time::Instant::now() < end,
            "expected {count} blocked owned statements, saw {waiting}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn stale_after_provider(
    f: &Fixture,
    service: &Arc<StripeBillingService<PostgresBillingStore>>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    w: &Wire,
) {
    let id = seed(f).await;
    let operation = Uuid::new_v4();
    let profile = profile(f);
    let worker = service.clone();
    let publisher = publisher
        .with_audit_context(AuditContext::actor("service:portal", "stale-provider"))
        .unwrap();
    w.pause.store(true, Ordering::SeqCst);
    let task = tokio::task::spawn_local(async move {
        worker
            .expire_bound_created_payment_link(&publisher, &profile, operation, id)
            .await
    });
    w.entered.notified().await;
    tokio::time::sleep(Duration::from_millis(5100)).await;
    w.pause.store(false, Ordering::SeqCst);
    w.resume.notify_one();
    assert!(matches!(
        task.await.unwrap().unwrap(),
        BoundPaymentLinkExpirationOutcome::Unresolved { .. }
    ));
    assert!(matches!(
        f.store
            .inspect_payment_link_expiration(
                &f.scope,
                STRIPE_API_VERSION,
                operation,
                id,
                &json!(["price_owned"])
            )
            .await
            .unwrap(),
        Some(PaymentLinkExpirationStatus::Pending { .. })
    ));
    assert!(
        owner_sql(
            f,
            &format!(
                "UPDATE fl_billing.records SET data=data WHERE kind='payment_link' AND id='{id}'"
            )
        )
        .await
        .is_err()
    );
}
#[tokio::test]
#[ignore = "owned schema16->17 upgrade with non-superuser migration owner"]
async fn expiration_migration_backfills_existing_rows() {
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    let name = format!("fl_plink_exp_upgrade_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE ROLE {name} LOGIN NOINHERIT NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE {name} OWNER {name}"
    )))
    .execute(&admin)
    .await
    .unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*admin.connect_options())
                .clone()
                .database(&name)
                .username(&name)
                .password("owned-fixture"),
        )
        .await
        .unwrap();
    let run = pool.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { migration_backfill(&run).await }).await
        })
        .await;
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {name} WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {name}")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    result.unwrap();
}
async fn migration_backfill(pool: &PgPool) {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../billing-base/migrations");
    let mut files = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|v| v == "sql"))
        .collect::<Vec<_>>();
    files.sort_by_key(|p| {
        p.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .parse::<u32>()
            .unwrap()
    });
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(
        "CREATE SCHEMA fl_billing; SELECT set_config('forklaunch.tenant_id','owned-upgrade',true)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    for file in &files {
        if file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("17_")
        {
            continue;
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(std::fs::read_to_string(file).unwrap()))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let id = Uuid::new_v4();
    let binding = Uuid::new_v4();
    sqlx::query("INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data) VALUES('owned-upgrade','durable','payment_link',$1,$2)").bind(id.to_string()).bind(sqlx::types::Json(json!({"id":id.to_string()}))).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO fl_billing.provider_bindings(tenant_id,kind,public_id,binding_id,storage_id,deployment_label,account_id,livemode,provider_id) VALUES('owned-upgrade','payment_link',$1::uuid,$2::uuid,$1,'owned','acct_owned',false,'plink_existing')").bind(id.to_string()).bind(binding.to_string()).execute(&mut *tx).await.unwrap();
    let migration = files
        .iter()
        .find(|p| p.file_name().unwrap().to_str().unwrap().starts_with("17_"))
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        std::fs::read_to_string(migration).unwrap(),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let guards:i64=sqlx::query_scalar("SELECT count(*) FROM fl_billing.payment_link_expiration_dependency_guards WHERE tenant_id='owned-upgrade' AND claims=0").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(guards, 2);
    assert_eq!(
        sqlx::query("UPDATE fl_billing.records SET data=data WHERE id=$1")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
    assert_eq!(sqlx::query("UPDATE fl_billing.provider_bindings SET binding_id=gen_random_uuid() WHERE binding_id=$1::uuid").bind(binding.to_string()).execute(&mut *tx).await.unwrap().rows_affected(),1);
    tx.commit().await.unwrap();
}
async fn denied_scope_and_grants(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    publisher: &TrustedPaymentLinkExpirationPublisher,
    client: &StripeBillingClient,
    w: &Wire,
    id: Uuid,
) {
    let before = w.calls.load(Ordering::SeqCst);
    let profile =
        BoundPaymentLinkExpirationProfile::trusted(f.scope.clone(), vec!["price_other".into()])
            .unwrap();
    assert!(
        service
            .expire_bound_created_payment_link(publisher, &profile, Uuid::new_v4(), id)
            .await
            .is_err()
    );
    let foreign = Arc::new(
        PostgresBillingStore::new(
            f.pool.clone(),
            TenantContext::new("foreign-expiration").unwrap(),
            Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
            AuditContext::actor("service:portal", "foreign"),
        )
        .await
        .unwrap(),
    );
    assert!(
        foreign
            .inspect_payment_link_expiration(
                &f.scope,
                STRIPE_API_VERSION,
                Uuid::new_v4(),
                id,
                &json!(["price_owned"])
            )
            .await
            .unwrap()
            .is_none()
    );
    let foreign = StripeBillingService::new(BaseBillingService::new(foreign), client.clone());
    assert!(
        foreign
            .expire_bound_created_payment_link(publisher, &profile, Uuid::new_v4(), id)
            .await
            .is_err()
    );
    let role = f.pool.connect_options().get_username().to_owned();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE SELECT ON fl_billing.payment_link_expiration_intents FROM {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        service
            .validate_bound_payment_link_read_configuration()
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT SELECT ON fl_billing.payment_link_expiration_intents TO {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    service
        .validate_bound_payment_link_read_configuration()
        .await
        .unwrap();
    assert_eq!(before, w.calls.load(Ordering::SeqCst));
}
