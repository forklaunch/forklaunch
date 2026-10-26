#![cfg(feature = "postgres-identity")]
#[path = "support/bound_portal_fixture.rs"]
mod fixture;
use chrono::{DateTime, Utc};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind,
    postgres::{
        PortalCreationClaim, PostgresBillingStore, StripeBindingScope, TrustedPortalPublisher,
    },
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
use sqlx::{PgPool, postgres::PgPoolOptions};
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
async fn audit_wait(admin: &PgPool) {
    for _ in 0..200 {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%INSERT INTO fl_billing.sensitive_read_audit%')").fetch_one(admin).await.unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("read did not reach blocked audit");
}
#[tokio::test]
#[ignore = "owned isolated PostgreSQL and zero-I/O provider trap"]
async fn stored_portal_provenance_ttl_and_mutation_barriers() {
    let fixture = Arc::new(Fixture::new().await);
    let run = fixture.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(fixture).ok().unwrap().cleanup().await;
    result.unwrap();
}
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
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
    let (operation, id, record) =
        seed(f, &publisher, Utc::now(), chrono::Duration::minutes(5)).await;
    let read = service
        .get_stored_bound_portal_session(&profile(f), id)
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(&read.record).unwrap(), record);
    assert!(read.observed_at >= read.record.created_at.unwrap());
    assert!(
        service
            .get_stored_bound_portal_session(&profile(f), Uuid::new_v4())
            .await
            .is_err()
    );
    let other = StripeBillingService::new(
        BaseBillingService::new(Arc::new(
            f.store
                .with_audit_context(AuditContext::actor("other", "read"))
                .unwrap(),
        )),
        client.clone(),
    );
    assert!(
        other
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    let foreign = PostgresBillingStore::new(
        f.pool.clone(),
        TenantContext::new("other-tenant").unwrap(),
        Arc::new(FieldEncryptor::new("owned-portal-foundation-key").unwrap()),
        AuditContext::actor("service:portal", "read"),
    )
    .await
    .unwrap();
    let foreign =
        StripeBillingService::new(BaseBillingService::new(Arc::new(foreign)), client.clone());
    assert!(
        foreign
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    for changed in [
        BoundPortalCreationProfile::trusted(
            f.scope.clone(),
            "bpc_changed".into(),
            "https://example.test/return".into(),
        )
        .unwrap(),
        BoundPortalCreationProfile::trusted(
            f.scope.clone(),
            "bpc_owned".into(),
            "https://example.test/changed".into(),
        )
        .unwrap(),
        BoundPortalCreationProfile::trusted(
            StripeBindingScope::administrative("different".into(), "acct_owned".into(), false)
                .unwrap(),
            "bpc_owned".into(),
            "https://example.test/return".into(),
        )
        .unwrap(),
    ] {
        assert!(
            service
                .get_stored_bound_portal_session(&changed, id)
                .await
                .is_err()
        );
    }
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::BillingPortal, id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .read_stored_portal_creation(
                &binding,
                "wrong-api-version",
                "bpc_owned",
                "https://example.test/return"
            )
            .await
            .is_err()
    );
    let managed = StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        StripeBillingClient::managed(&address, Uuid::new_v4().to_string(), "key".into()).unwrap(),
    );
    assert!(
        managed
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    for (created, duration) in [
        (
            Utc::now() - chrono::Duration::minutes(6),
            chrono::Duration::minutes(5),
        ),
        (
            Utc::now() + chrono::Duration::minutes(1),
            chrono::Duration::minutes(5),
        ),
        (Utc::now(), chrono::Duration::minutes(6)),
    ] {
        let (_, bad, _) = seed(f, &publisher, created, duration).await;
        assert!(
            service
                .get_stored_bound_portal_session(&profile(f), bad)
                .await
                .is_err()
        );
    }
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    // A correctly formed original5min receipt with only2sec left must expire during audit wait.
    let (_, short, _) = seed(
        f,
        &publisher,
        Utc::now() - chrono::Duration::minutes(5) + chrono::Duration::seconds(2),
        chrono::Duration::minutes(5),
    )
    .await;
    let mut blocker = f.owner.begin().await.unwrap();
    sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let policy = profile(f);
    let call = service.get_stored_bound_portal_session(&policy, short);
    tokio::pin!(call);
    tokio::select! {()=audit_wait(&admin)=>{},result=&mut call=>panic!("must wait {}",result.is_ok())}
    tokio::time::sleep(Duration::from_millis(2100)).await;
    blocker.rollback().await.unwrap();
    assert_eq!(
        call.await.err().unwrap().code,
        "billing_portal_local_session_expired"
    );
    // Changes during audit cannot be hidden by a later unchanged plaintext.
    for sql in [
        format!(
            "UPDATE fl_billing.records SET data=data WHERE kind='billing_portal' AND id='{id}'"
        ),
        format!(
            "UPDATE fl_billing.provider_bindings SET provider_id=provider_id WHERE public_id='{id}'::uuid"
        ),
        "UPDATE fl_billing.records SET data=data WHERE kind='customer'".into(),
    ] {
        let mut blocker = f.owner.begin().await.unwrap();
        sqlx::query("LOCK TABLE fl_billing.sensitive_read_audit IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let policy = profile(f);
        let call = service.get_stored_bound_portal_session(&policy, id);
        tokio::pin!(call);
        tokio::select! {()=audit_wait(&admin)=>{},result=&mut call=>panic!("must wait {}",result.is_ok())}
        let mut mutation = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
            .execute(&mut *mutation)
            .await
            .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.clone()))
            .execute(&mut *mutation)
            .await
            .unwrap();
        mutation.commit().await.unwrap();
        blocker.rollback().await.unwrap();
        assert!(call.await.is_err());
    }
    assert!(
        service
            .get_stored_bound_portal_session(&profile(f), id)
            .await
            .is_err()
    );
    // Inspection remains IDs-only even after expiry/Customer changes.
    assert!(matches!(
        f.store
            .inspect_bound_portal_creation(operation)
            .await
            .unwrap(),
        Some(forklaunch_billing_base::services::postgres::PortalCreationStatus::Completed { .. })
    ));
    assert_eq!(f.count("portal_dependency_claims").await, 0);
    // Existing fixture helper: no claimed rows means mutation is allowed; do not invent a retained barrier.
    f.denied("DELETE FROM fl_billing.portal_creation_intents", "portal-a")
        .await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    server.abort();
    let _ = server.await;
    admin.close().await;
    publisher_pool.close().await;
}
