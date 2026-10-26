#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_portal_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use chrono::{DateTime, Utc};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{PortalCreationClaim, PostgresBillingStore, TrustedPortalPublisher},
};
use forklaunch_billing_stripe::{
    bound_portal_creation::BoundPortalCreationProfile,
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
fn profile(f: &Fixture) -> BoundPortalCreationProfile {
    BoundPortalCreationProfile::trusted(
        f.scope.clone(),
        "bpc_owned".into(),
        "https://example.test/return".into(),
    )
    .unwrap()
}
async fn seed(
    f: &Fixture,
    publisher: &TrustedPortalPublisher,
    created: DateTime<Utc>,
    duration: chrono::Duration,
) -> (Uuid, Uuid, Value) {
    // Explicit trusted publisher fixture, not a claim that Stripe created this historical session.
    let operation = Uuid::new_v4();
    let snapshot = f
        .store
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let payload = json!({"profile":"direct_portal_v1","customer":f.customer.to_string(),"fields":{"customer":"cus_owned","configuration":"bpc_owned","return_url":"https://example.test/return"}});
    let PortalCreationClaim::Reserved(ticket) = f
        .store
        .reserve_portal_creation(snapshot, operation, &payload, || Ok(()))
        .await
        .unwrap()
    else {
        panic!("new reservation")
    };
    let public = ticket.public_id();
    let provider = json!({"id":format!("bps_{}",public.simple()),"object":"billing_portal.session","livemode":false,"customer":"cus_owned","configuration":"bpc_owned","return_url":"https://example.test/return","url":"https://billing.stripe.com/p/session/private","created":created.timestamp(),"on_behalf_of":null,"flow":null});
    let record = json!({"id":public.to_string(),"customerId":f.customer.to_string(),"uri":provider["url"],"providerFields":provider,"createdAt":created,"updatedAt":created,"expiresAt":created+duration});
    publisher
        .complete(ticket, record.clone(), || Ok(()))
        .await
        .unwrap();
    (operation, public, record)
}
#[tokio::test]
#[ignore = "owned restricted PostgreSQL and zero-I/O provider trap"]
async fn local_portal_expiration_is_permanent_actor_bound_and_atomic() {
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
    use forklaunch_billing_stripe::bound_portal_reads::PortalLocalExpirationOutcome;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().fallback(move || {
                observed.fetch_add(1, Ordering::SeqCst);
                async { axum::http::StatusCode::INTERNAL_SERVER_ERROR }
            }),
        )
        .await
        .unwrap();
    });
    let client = StripeBillingClient::with_transport(
        "owned-key".into(),
        ProviderHttpClient::new(&address, Duration::from_secs(1), 1024).unwrap(),
    )
    .unwrap();
    let service =
        StripeBillingService::new(BaseBillingService::new(f.store.clone()), client.clone());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let publisher = f
        .store
        .trusted_portal_publisher(publisher_pool.clone())
        .await
        .unwrap();
    service
        .validate_portal_local_expiration_configuration()
        .await
        .unwrap();
    let (operation, id, record) =
        seed(f, &publisher, Utc::now(), chrono::Duration::minutes(5)).await;
    // Owned schema-15 upgrade simulation with an existing sealed Portal receipt.
    sqlx::raw_sql("DROP TABLE fl_billing.portal_local_expirations; DROP FUNCTION fl_billing.guard_portal_expiration_history(); DELETE FROM fl_billing._sqlx_migrations WHERE version=16").execute(&f.owner).await.unwrap();
    PostgresBillingStore::migrate(&f.owner).await.unwrap();
    PostgresBillingStore::migrate(&f.owner).await.unwrap();
    let role = f.pool.connect_options().get_username().to_owned();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT ALL ON fl_billing.portal_local_expirations TO {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(
            service
                .get_stored_bound_portal_session(&profile(f), id)
                .await
                .unwrap()
                .record
        )
        .unwrap(),
        record
    );
    assert!(
        !service
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .unwrap()
    );
    let other = StripeBillingService::new(
        BaseBillingService::new(Arc::new(
            f.store
                .with_audit_context(AuditContext::actor("other", "expire"))
                .unwrap(),
        )),
        client.clone(),
    );
    assert!(
        other
            .expire_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    let foreign = PostgresBillingStore::new(
        f.pool.clone(),
        TenantContext::new("other-tenant").unwrap(),
        Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
        AuditContext::actor("service:portal", "expire"),
    )
    .await
    .unwrap();
    let foreign =
        StripeBillingService::new(BaseBillingService::new(Arc::new(foreign)), client.clone());
    assert!(
        foreign
            .expire_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    let changed = BoundPortalCreationProfile::trusted(
        f.scope.clone(),
        "bpc_changed".into(),
        "https://example.test/return".into(),
    )
    .unwrap();
    assert!(
        service
            .expire_stored_bound_portal_session(&changed, id)
            .await
            .is_err()
    );
    let a = profile(f);
    let b = profile(f);
    let (a, b) = tokio::join!(
        service.expire_stored_bound_portal_session(&a, id),
        service.expire_stored_bound_portal_session(&b, id)
    );
    assert!(matches!(a.unwrap(), PortalLocalExpirationOutcome::Expired));
    assert!(matches!(b.unwrap(), PortalLocalExpirationOutcome::Expired));
    assert!(
        service
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .unwrap()
    );
    assert!(
        service
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    assert!(
        other
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .is_err()
    );
    assert!(
        service
            .inspect_stored_bound_portal_expiration(&changed, id)
            .await
            .is_err()
    );
    assert!(
        f.store
            .inspect_bound_portal_creation(operation)
            .await
            .unwrap()
            .is_some()
    );
    let count = f.count("portal_local_expirations").await;
    assert_eq!(count, 1);
    // Restoring the exact encrypted record cannot remove the independent tombstone.
    touch(f, "billing_portal", id).await;
    assert!(
        service
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    for statement in [
        "UPDATE fl_billing.portal_local_expirations SET actor_id='other'",
        "DELETE FROM fl_billing.portal_local_expirations",
        "TRUNCATE fl_billing.portal_local_expirations",
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = sqlx::raw_sql(statement).execute(&mut *tx).await;
        assert!(result.is_err() || result.unwrap().rows_affected() == 0);
        tx.rollback().await.unwrap();
    }
    assert_eq!(record["id"], id.to_string());
    let (_, expired, _) = seed(
        f,
        &publisher,
        Utc::now() - chrono::Duration::minutes(6),
        chrono::Duration::minutes(5),
    )
    .await;
    assert!(
        service
            .expire_stored_bound_portal_session(&profile(f), expired)
            .await
            .is_err()
    );
    assert!(
        !service
            .inspect_stored_bound_portal_expiration(&profile(f), expired)
            .await
            .unwrap()
    );
    expiry_during_audit(f, &publisher).await;
    faults(f, &publisher, &service).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    publisher_pool.close().await;
    server.abort();
    let _ = server.await;
}
#[allow(clippy::too_many_lines)]
async fn faults(
    f: &Fixture,
    publisher: &TrustedPortalPublisher,
    service: &StripeBillingService<PostgresBillingStore>,
) {
    use forklaunch_billing_base::services::BillingKind;
    let (_, id, _) = seed(f, publisher, Utc::now(), chrono::Duration::minutes(5)).await;
    let role = f.pool.connect_options().get_username().to_owned();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.portal_local_expirations FROM {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    let before = f.count("sensitive_read_audit").await;
    assert!(
        service
            .expire_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    let after = f.count("sensitive_read_audit").await;
    assert_eq!(before, after);
    assert!(
        !service
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .unwrap()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.portal_local_expirations TO {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::BillingPortal, id)
        .await
        .unwrap()
        .unwrap();
    let options = f.pool.connect_options();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*options)
                .clone()
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(sqlx::postgres::PgSslMode::Disable),
        )
        .await
        .unwrap();
    let store = PostgresBillingStore::new(
        pool.clone(),
        TenantContext::new("portal-a").unwrap(),
        Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
        AuditContext::actor("service:portal", "lost-ack"),
    )
    .await
    .unwrap();
    armed.store(true, Ordering::SeqCst);
    assert!(
        !store
            .expire_stored_portal_creation(
                &binding,
                STRIPE_API_VERSION,
                "bpc_owned",
                "https://example.test/return"
            )
            .await
            .unwrap()
    );
    assert!(
        service
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .unwrap()
    );
    pool.close().await;
    proxy.abort();
    let (_, stale, _) = seed(f, publisher, Utc::now(), chrono::Duration::minutes(5)).await;
    touch(f, "customer", f.customer).await;
    assert!(
        service
            .expire_stored_bound_portal_session(&profile(f), stale)
            .await
            .is_err()
    );
    assert!(
        !service
            .inspect_stored_bound_portal_expiration(&profile(f), stale)
            .await
            .unwrap()
    );
    // Completed expiration remains a historical receipt even after dependency changes.
    assert!(
        service
            .inspect_stored_bound_portal_expiration(&profile(f), id)
            .await
            .unwrap()
    );
}
async fn expiry_during_audit(f: &Fixture, publisher: &TrustedPortalPublisher) {
    use forklaunch_billing_base::services::BillingKind;
    let (_, id, _) = seed(
        f,
        publisher,
        Utc::now() - chrono::Duration::minutes(5) + chrono::Duration::seconds(2),
        chrono::Duration::minutes(5),
    )
    .await;
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::BillingPortal, id)
        .await
        .unwrap()
        .unwrap();
    let mut blocker = f.owner.begin().await.unwrap();
    sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let store = f.store.clone();
    let expiration = tokio::task::spawn_local(async move {
        store
            .expire_stored_portal_creation(
                &binding,
                STRIPE_API_VERSION,
                "bpc_owned",
                "https://example.test/return",
            )
            .await
    });
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    for i in 0..200 {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%INSERT INTO fl_billing.sensitive_read_audit%')").fetch_one(&admin).await.unwrap();
        if waiting {
            break;
        }
        assert!(i < 199);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    admin.close().await;
    let mut collision = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *collision)
        .await
        .unwrap();
    assert!(sqlx::query("SELECT id FROM fl_billing.records WHERE tenant_id='portal-a' AND kind='customer' AND id=$1 FOR UPDATE NOWAIT").bind(f.customer.to_string()).fetch_one(&mut *collision).await.is_err());
    collision.rollback().await.unwrap();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    blocker.rollback().await.unwrap();
    assert!(expiration.await.unwrap().is_err());
    assert!(
        !f.store
            .inspect_portal_local_expiration(
                &f.scope,
                id,
                STRIPE_API_VERSION,
                "bpc_owned",
                "https://example.test/return"
            )
            .await
            .unwrap()
    );
}

async fn touch(f: &Fixture, kind: &str, id: Uuid) {
    let mut tx = f.owner.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(sqlx::query("UPDATE fl_billing.records SET data=data WHERE tenant_id='portal-a' AND kind=$1 AND id=$2").bind(kind).bind(id.to_string()).execute(&mut *tx).await.unwrap().rows_affected(),1);
    tx.commit().await.unwrap();
}
