#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_plan_creation_fixture.rs"]
mod fixture;
use fixture::{Fixture, plan, product, request};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, postgres::PostgresBillingStore,
};
use forklaunch_billing_stripe::{
    bound_plan_creation::{BoundPlanCreationOutcome as Outcome, BoundPlanCreationProfile},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use serde_json::json;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use uuid::Uuid;
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn bound_plan_creation_matrix() {
    let f = Fixture::new().await;
    success(&f).await;
    concurrent_interruption(&f).await;
    denied_provider_scope(&f).await;
    expiry_after_row_lock(&f).await;
    invalid_evidence(&f).await;
    atomic_and_stale(&f).await;
    lost_commit_acknowledgement(&f).await;
    f.cleanup().await;
}
// Keep the receipt, alias and tenant assertions for one operation together.
#[allow(clippy::too_many_lines)]
async fn success(f: &Fixture) {
    let op = Uuid::new_v4();
    let before = f.posts();
    let Outcome::Completed { record, .. } = f
        .service
        .create_bound_plan(&f.profile, op, request())
        .await
        .unwrap()
    else {
        panic!("expected complete")
    };
    let id = Uuid::parse_str(&record.id).unwrap();
    assert_ne!(record.id, record.external_id);
    assert!(record.external_id.starts_with("custom-plan-"));
    assert_eq!(record.provider_fields.as_ref().unwrap(), &product());
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::Plan, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.provider_id(), record.external_id);
    assert!(
        f.store
            .get(Scope::Cache, BillingKind::Plan, &record.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    let mut changed = request();
    changed.unit_amount += 1;
    assert!(
        f.service
            .create_bound_plan(&f.profile, op, changed)
            .await
            .is_err()
    );
    let profile = BoundPlanCreationProfile::trusted(
        f.scope.clone(),
        vec!["custom-product".into(), "another-product".into()],
    )
    .unwrap();
    assert!(
        f.service
            .create_bound_plan(&profile, op, request())
            .await
            .is_err()
    );
    f.store
        .patch(
            Scope::Durable,
            BillingKind::Plan,
            &record.id,
            json!({"externalId":"attacker","providerFields":{"id":"attacker"}}),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let Outcome::Completed {
        record: historical, ..
    } = f
        .service
        .inspect_bound_plan_creation(op)
        .await
        .unwrap()
        .unwrap()
    else {
        panic!("receipt")
    };
    assert_eq!(historical.external_id, record.external_id);
    assert_eq!(
        f.store
            .stripe_binding_by_public_id(&f.scope, BillingKind::Plan, id)
            .await
            .unwrap()
            .unwrap()
            .provider_id(),
        record.external_id
    );
    journal_integrity(f, op).await;
    let other = f
        .store
        .with_audit_context(AuditContext::actor("other", "test"))
        .unwrap();
    assert!(other.inspect_plan_creation(op).await.is_err());
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','foreign-tenant',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_billing.plan_creation_intents")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    tx.rollback().await.unwrap();
}
async fn invalid_evidence(f: &Fixture) {
    for bad in [
        json!(null),
        json!({"id":"custom-product","object":"product","livemode":true}),
        json!({"id":"wrong","object":"product","livemode":false}),
        json!({"id":"custom-product","object":"product","livemode":false,"deleted":true}),
        json!({"id":"custom-product","object":"product","livemode":false,"deleted":null}),
    ] {
        *f.provider.product.lock().unwrap() = bad;
        let op = Uuid::new_v4();
        let before = f.posts();
        let counts = f.counts().await;
        assert!(matches!(
            f.service
                .create_bound_plan(&f.profile, op, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.posts(), before);
        assert_eq!(f.counts().await, counts);
    }
    *f.provider.product.lock().unwrap() = product();
    for (key, bad) in [
        ("id", json!(null)),
        ("object", json!("price")),
        ("livemode", json!(true)),
        ("amount", json!(1201)),
        ("amount", json!(1200.5)),
        ("amount", json!("1200")),
        ("currency", json!("eur")),
        ("interval", json!("year")),
        ("product", json!("other")),
        ("active", json!(false)),
    ] {
        let mut response = plan("custom-plan-invalid");
        response[key] = bad;
        *f.provider.response.lock().unwrap() = Some(response);
        let op = Uuid::new_v4();
        let before = f.posts();
        let counts = f.counts().await;
        assert!(matches!(
            f.service
                .create_bound_plan(&f.profile, op, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.posts(), before + 1);
        assert_eq!(f.counts().await, counts);
        assert!(matches!(
            f.service
                .create_bound_plan(&f.profile, op, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.posts(), before + 1);
    }
    *f.provider.response.lock().unwrap() = None;
    *f.provider.readback.lock().unwrap() = Some(plan("wrong-readback"));
    let counts = f.counts().await;
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, Uuid::new_v4(), request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, counts);
    *f.provider.readback.lock().unwrap() = None;
    f.provider.delay.store(2100, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let before = f.posts();
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    f.provider.delay.store(0, Ordering::SeqCst);
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 1);
}
async fn atomic_and_stale(f: &Fixture) {
    let before = f.counts().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, Uuid::new_v4(), request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.provider_binding_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    let function = format!("plan_wait_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fl_billing.{function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{}' THEN PERFORM pg_sleep(5.1); END IF; RETURN NEW; END $$; CREATE TRIGGER {function} BEFORE INSERT ON fl_billing.records FOR EACH ROW EXECUTE FUNCTION fl_billing.{function}()",f.tenant.id()))).execute(&f.owner).await.unwrap();
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, Uuid::new_v4(), request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {function} ON fl_billing.records; DROP FUNCTION fl_billing.{function}()"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
}

// Keep the sequential fixture lifecycle and its assertions together.
#[allow(clippy::too_many_lines)]
async fn lost_commit_acknowledgement(f: &Fixture) {
    let options = (*f.owner.connect_options()).clone();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            options
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(sqlx::postgres::PgSslMode::Disable)
                .username(&f.role)
                .password("owned-fixture"),
        )
        .await
        .unwrap();
    let store = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            f.tenant.clone(),
            Arc::new(
                forklaunch_core::persistence::encryption::FieldEncryptor::new(
                    "owned-bound-create-encryption-key",
                )
                .unwrap(),
            ),
            AuditContext::actor("service:bound-create", "lost-commit-ack"),
        )
        .await
        .unwrap(),
    );
    let service = StripeBillingService::new(BaseBillingService::new(store), f.client.clone());
    let reservation_op = Uuid::new_v4();
    let before_reservation = f.posts();
    armed.store(true, Ordering::SeqCst);
    assert!(matches!(
        service
            .create_bound_plan(&f.profile, reservation_op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    assert_eq!(f.posts(), before_reservation);
    assert!(matches!(
        f.service
            .inspect_bound_plan_creation(reservation_op)
            .await
            .unwrap(),
        Some(Outcome::Unresolved { .. })
    ));
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, reservation_op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before_reservation);
    let op = Uuid::new_v4();
    let scope = f.scope.clone();
    let before = f.posts();
    f.provider.pause.store(true, Ordering::SeqCst);
    let task = tokio::spawn(async move {
        let profile =
            forklaunch_billing_stripe::bound_plan_creation::BoundPlanCreationProfile::trusted(
                scope,
                vec!["custom-product".into()],
            )
            .unwrap();
        service.create_bound_plan(&profile, op, request()).await
    });
    f.provider.entered.notified().await;
    armed.store(true, Ordering::SeqCst);
    f.provider.resume.notify_one();
    assert!(matches!(
        task.await.unwrap().unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    f.provider.pause.store(false, Ordering::SeqCst);
    assert!(matches!(
        f.service.inspect_bound_plan_creation(op).await.unwrap(),
        Some(Outcome::Completed { .. })
    ));
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    pool.close().await;
    proxy.abort();
}

async fn expiry_after_row_lock(f: &Fixture) {
    let counts = f.counts().await;
    let op = Uuid::new_v4();
    let service = f.service.clone();
    let scope = f.scope.clone();
    f.provider.pause.store(true, Ordering::SeqCst);
    let task = tokio::spawn(async move {
        let profile =
            forklaunch_billing_stripe::bound_plan_creation::BoundPlanCreationProfile::trusted(
                scope,
                vec!["custom-product".into()],
            )
            .unwrap();
        service.create_bound_plan(&profile, op, request()).await
    });
    f.provider.entered.notified().await;
    let mut lock = f.owner.begin().await.unwrap();
    sqlx::query("SELECT operation_id FROM fl_billing.plan_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid FOR UPDATE").bind(f.tenant.id()).bind(op.to_string()).execute(&mut *lock).await.unwrap();
    f.provider.resume.notify_one();
    f.provider.pause.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(5100)).await;
    lock.rollback().await.unwrap();
    assert!(matches!(
        task.await.unwrap().unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, counts);
    assert!(matches!(
        f.service.inspect_bound_plan_creation(op).await.unwrap(),
        Some(Outcome::Unresolved { .. })
    ));
}

async fn denied_provider_scope(f: &Fixture) {
    for (account, mode) in [("acct_other", false), ("acct_owned", true)] {
        let scope =
            forklaunch_billing_base::services::postgres::StripeBindingScope::administrative(
                "owned-deployment".into(),
                account.into(),
                mode,
            )
            .unwrap();
        let profile =
            forklaunch_billing_stripe::bound_plan_creation::BoundPlanCreationProfile::trusted(
                scope,
                vec!["custom-product".into()],
            )
            .unwrap();
        let operation = Uuid::new_v4();
        let before = f.posts();
        assert!(matches!(
            f.service
                .create_bound_plan(&profile, operation, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.posts(), before);
        assert!(matches!(
            f.service
                .inspect_bound_plan_creation(operation)
                .await
                .unwrap(),
            Some(Outcome::Unresolved { .. })
        ));
    }
}

async fn concurrent_interruption(f: &Fixture) {
    let op = Uuid::new_v4();
    let before = f.posts();
    let counts = f.counts().await;
    f.provider.pause.store(true, Ordering::SeqCst);
    let service = f.service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        let profile =
            BoundPlanCreationProfile::trusted(scope, vec!["custom-product".into()]).unwrap();
        service.create_bound_plan(&profile, op, request()).await
    });
    f.provider.entered.notified().await;
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    task.abort();
    let _ = task.await;
    f.provider.resume.notify_one();
    f.provider.pause.store(false, Ordering::SeqCst);
    assert!(matches!(
        f.service
            .create_bound_plan(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    assert_eq!(f.counts().await, counts);
}

async fn journal_integrity(f: &Fixture, op: Uuid) {
    let (intent,receipt):(String,String)=sqlx::query_as("SELECT intent_ciphertext,receipt_ciphertext FROM fl_billing.plan_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid").bind(f.tenant.id()).bind(op.to_string()).fetch_one(&f.owner).await.unwrap();
    assert!(!intent.contains("custom-product"));
    assert!(!receipt.contains("custom-plan"));
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("UPDATE fl_billing.plan_creation_intents SET account_id='acct_wrong' WHERE operation_id=$1::uuid").bind(op.to_string()).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("DELETE FROM fl_billing.plan_creation_intents")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    assert_eq!(
        sqlx::query("UPDATE fl_billing.plan_creation_audit SET actor_id='wrong'")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
}
