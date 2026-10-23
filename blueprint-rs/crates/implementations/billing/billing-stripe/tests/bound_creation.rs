#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[path = "support/bound_creation_fixture.rs"]
mod fixture;
use fixture::{Fixture, payload, request};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope,
    postgres::{PaymentLinkCreationClaim, PaymentLinkCreationStatus, PostgresBillingStore},
};
use forklaunch_billing_stripe::{
    bound_creation::BoundPaymentLinkCreationOutcome as Outcome,
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
async fn direct_bound_creation_journal_matrix() {
    let f = Fixture::new().await;
    hostile_reservation_input(&f).await;
    completed_and_conflicts(&f).await;
    denied_profiles(&f).await;
    denied_provider_scope(&f).await;
    concurrent_and_interrupted(&f).await;
    malformed_and_timeout(&f).await;
    atomic_failure_and_late_context(&f).await;
    expiry_after_row_lock(&f).await;
    tenant_and_immutable_journal(&f).await;
    lost_commit_acknowledgement(&f).await;
    f.cleanup().await;
}
// Keep the sequential fixture lifecycle and its assertions together.
#[allow(clippy::too_many_lines)]
async fn completed_and_conflicts(f: &Fixture) {
    let op = Uuid::new_v4();
    let before = f.posts();
    let Outcome::Completed { record, .. } = f
        .service
        .create_bound_payment_link(&f.profile, op, request())
        .await
        .unwrap()
    else {
        panic!("expected completed");
    };
    let public = Uuid::parse_str(&record.id).unwrap();
    assert!(!public.is_nil());
    let provider = record.provider_fields.as_ref().unwrap()["id"]
        .as_str()
        .unwrap();
    assert!(provider.starts_with("plink_created_"));
    assert_ne!(record.id, provider);
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::PaymentLink, public)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.storage_id(), record.id);
    assert_eq!(binding.provider_id(), provider);
    assert!(
        f.store
            .get(Scope::Cache, BillingKind::PaymentLink, &record.id)
            .await
            .unwrap()
            .is_none()
    );
    f.store
        .patch(
            Scope::Durable,
            BillingKind::PaymentLink,
            &record.id,
            json!({"providerFields":{"id":"plink_attacker"}}),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let Outcome::Completed { record: again, .. } = f
        .service
        .create_bound_payment_link(&f.profile, op, request())
        .await
        .unwrap()
    else {
        panic!("receipt lost");
    };
    assert_eq!(again, record);
    assert_eq!(f.posts(), before + 1);
    let changed_client = f
        .client
        .clone()
        .with_api_version("2020-01-01".into())
        .unwrap();
    let changed_service =
        StripeBillingService::new(BaseBillingService::new(f.store.clone()), changed_client);
    assert!(
        changed_service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .is_err()
    );
    let changed_profile =
        forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
            f.scope.clone(),
            vec!["price_owned".into(), "price_extra".into()],
        )
        .unwrap();
    assert!(
        f.service
            .create_bound_payment_link(&changed_profile, op, request())
            .await
            .is_err()
    );
    assert_eq!(f.posts(), before + 1);
    let mut changed = request();
    changed.amount = 99.0;
    assert!(
        f.service
            .create_bound_payment_link(&f.profile, op, changed)
            .await
            .is_err()
    );
    assert_eq!(f.posts(), before + 1);
    assert_eq!(
        f.store
            .stripe_binding_by_public_id(&f.scope, BillingKind::PaymentLink, public)
            .await
            .unwrap()
            .unwrap()
            .provider_id(),
        provider
    );
    let encrypted:(String,String)=sqlx::query_as("SELECT intent_ciphertext,receipt_ciphertext FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid").bind(f.tenant.id()).bind(op.to_string()).fetch_one(&f.owner).await.unwrap();
    assert!(!encrypted.0.contains("price_owned"));
    assert!(!encrypted.1.contains("private-token"));
    let ciphertext: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id=$2")
            .bind(f.tenant.id())
            .bind(&record.id)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    PostgresBillingStore::migrate(&f.owner).await.unwrap();
    let after: sqlx::types::Json<Value> =
        sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id=$2")
            .bind(f.tenant.id())
            .bind(&record.id)
            .fetch_one(&f.owner)
            .await
            .unwrap();
    assert_eq!(after.0, ciphertext.0);
}
async fn denied_profiles(f: &Fixture) {
    let before = f.provider.requests.load(Ordering::SeqCst);
    let mut bad = request();
    bad.id = Some(Uuid::new_v4().to_string());
    assert!(
        f.service
            .create_bound_payment_link(&f.profile, Uuid::new_v4(), bad)
            .await
            .is_err()
    );
    for field in [
        "on_behalf_of",
        "transfer_data",
        "application_fee_amount",
        "application_fee_percent",
        "payment_intent_data",
        "subscription_data",
        "metadata",
        "unknown",
    ] {
        let mut bad = request();
        bad.provider_fields.as_mut().unwrap()[field] = json!({"unsafe":"acct_foreign"});
        assert!(
            f.service
                .create_bound_payment_link(&f.profile, Uuid::new_v4(), bad)
                .await
                .is_err()
        );
    }
    let managed = StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        StripeBillingClient::managed(&f.base, "instance".into(), "fixture-key".into()).unwrap(),
    );
    assert!(
        managed
            .create_bound_payment_link(&f.profile, Uuid::new_v4(), request())
            .await
            .is_err()
    );
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    for actor in [None, Some(" ".into())] {
        let mut audit = AuditContext::actor("unused", "actor-denial");
        audit.actor_id = actor;
        let store = Arc::new(f.store.with_audit_context(audit).unwrap());
        let service = StripeBillingService::new(BaseBillingService::new(store), f.client.clone());
        assert!(
            service
                .create_bound_payment_link(&f.profile, Uuid::new_v4(), request())
                .await
                .is_err()
        );
    }
}
async fn concurrent_and_interrupted(f: &Fixture) {
    let op = Uuid::new_v4();
    let before = f.posts();
    f.provider.pause.store(true, Ordering::SeqCst);
    let service = f.service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        let profile =
            forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
                scope,
                vec!["price_owned".into()],
            )
            .unwrap();
        service
            .create_bound_payment_link(&profile, op, request())
            .await
    });
    f.provider.entered.notified().await;
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    f.provider.resume.notify_one();
    assert!(matches!(
        task.await.unwrap().unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    // A crash before any provider request permanently consumes the reservation.
    let op = Uuid::new_v4();
    let claim = f
        .store
        .reserve_payment_link_creation(&f.scope, op, STRIPE_API_VERSION, &payload())
        .await
        .unwrap();
    assert!(matches!(claim, PaymentLinkCreationClaim::Reserved(_)));
    drop(claim);
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    // Cancellation after the POST reached the provider cannot issue another POST.
    let op = Uuid::new_v4();
    let service = f.service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        let profile =
            forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
                scope,
                vec!["price_owned".into()],
            )
            .unwrap();
        service
            .create_bound_payment_link(&profile, op, request())
            .await
    });
    f.provider.entered.notified().await;
    task.abort();
    let _ = task.await;
    f.provider.resume.notify_one();
    f.provider.pause.store(false, Ordering::SeqCst);
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 2);
}
async fn malformed_and_timeout(f: &Fixture) {
    let counts = f.counts().await;
    for body in [
        json!({}),
        json!({"id":"plink_wrong","object":"payment_link","livemode":true}),
        json!({"id":"plink_wrong","object":"customer","livemode":false}),
        json!({"id":"not_link","object":"payment_link","livemode":false}),
    ] {
        *f.provider.response.lock().unwrap() = Some(body);
        let op = Uuid::new_v4();
        let before = f.posts();
        assert!(matches!(
            f.service
                .create_bound_payment_link(&f.profile, op, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert!(matches!(
            f.service
                .inspect_bound_payment_link_creation(op)
                .await
                .unwrap(),
            Some(Outcome::Unresolved { .. })
        ));
        assert_eq!(f.posts(), before + 1);
        assert_eq!(f.counts().await, counts);
    }
    *f.provider.response.lock().unwrap() = None;
    f.provider.delay.store(2100, Ordering::SeqCst);
    let op = Uuid::new_v4();
    let before = f.posts();
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    f.provider.delay.store(0, Ordering::SeqCst);
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.posts(), before + 1);
    assert_eq!(f.counts().await, counts);
}
async fn atomic_failure_and_late_context(f: &Fixture) {
    let before = f.counts().await;
    let op = Uuid::new_v4();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.provider_binding_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert_eq!(f.counts().await, before);
    assert!(matches!(
        f.store.inspect_payment_link_creation(op).await.unwrap(),
        Some(PaymentLinkCreationStatus::Pending { .. })
    ));
    // Delay a write after the finalization row lock; the last freshness check rolls back all writes.
    let function = format!("{}_delay", f.role);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fl_billing.{function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{}' THEN PERFORM pg_sleep(5.1); END IF; RETURN NEW; END; $$; CREATE TRIGGER {function} BEFORE INSERT ON fl_billing.records FOR EACH ROW EXECUTE FUNCTION fl_billing.{function}()",f.tenant.id()))).execute(&f.owner).await.unwrap();
    let op = Uuid::new_v4();
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {function} ON fl_billing.records; DROP FUNCTION fl_billing.{function}()"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert_eq!(f.counts().await, before);
    // Deferred trigger runs during COMMIT after the final precommit check.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fl_billing.{function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{}' AND NEW.operation='complete' THEN PERFORM pg_sleep(5.1); END IF; RETURN NEW; END; $$; CREATE CONSTRAINT TRIGGER {function} AFTER INSERT ON fl_billing.payment_link_creation_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fl_billing.{function}()",f.tenant.id()))).execute(&f.owner).await.unwrap();
    let op = Uuid::new_v4();
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
            .await
            .unwrap(),
        Outcome::RecordedEvidenceNoLongerCurrent { .. }
    ));
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER {function} ON fl_billing.payment_link_creation_audit; DROP FUNCTION fl_billing.{function}()"))).execute(&f.owner).await.unwrap();
    assert!(matches!(
        f.service
            .inspect_bound_payment_link_creation(op)
            .await
            .unwrap(),
        Some(Outcome::Completed { .. })
    ));
}
async fn tenant_and_immutable_journal(f: &Fixture) {
    let op = Uuid::new_v4();
    assert!(matches!(
        f.store
            .reserve_payment_link_creation(&f.scope, op, STRIPE_API_VERSION, &payload())
            .await
            .unwrap(),
        PaymentLinkCreationClaim::Reserved(_)
    ));
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(format!("{}-foreign", f.role))
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_billing.payment_link_creation_intents WHERE operation_id=$1::uuid",
    )
    .bind(op.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(count, 0);
    tx.rollback().await.unwrap();
    let other = f
        .store
        .with_audit_context(AuditContext::actor("other-actor", "other-audit"))
        .unwrap();
    assert!(other.inspect_payment_link_creation(op).await.is_err());
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("UPDATE fl_billing.payment_link_creation_intents SET account_id='acct_attacker' WHERE operation_id=$1::uuid").bind(op.to_string()).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("DELETE FROM fl_billing.payment_link_creation_intents")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    assert_eq!(
        sqlx::query("UPDATE fl_billing.payment_link_creation_audit SET actor_id='attacker'")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
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
            .create_bound_payment_link(&f.profile, reservation_op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    assert_eq!(f.posts(), before_reservation);
    assert!(matches!(
        f.service
            .inspect_bound_payment_link_creation(reservation_op)
            .await
            .unwrap(),
        Some(Outcome::Unresolved { .. })
    ));
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, reservation_op, request())
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
            forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
                scope,
                vec!["price_owned".into()],
            )
            .unwrap();
        service
            .create_bound_payment_link(&profile, op, request())
            .await
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
        f.service
            .inspect_bound_payment_link_creation(op)
            .await
            .unwrap(),
        Some(Outcome::Completed { .. })
    ));
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, op, request())
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
            forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
                scope,
                vec!["price_owned".into()],
            )
            .unwrap();
        service
            .create_bound_payment_link(&profile, op, request())
            .await
    });
    f.provider.entered.notified().await;
    let mut lock = f.owner.begin().await.unwrap();
    sqlx::query("SELECT operation_id FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid FOR UPDATE").bind(f.tenant.id()).bind(op.to_string()).execute(&mut *lock).await.unwrap();
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
        f.service
            .inspect_bound_payment_link_creation(op)
            .await
            .unwrap(),
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
            forklaunch_billing_stripe::bound_creation::BoundPaymentLinkCreationProfile::trusted(
                scope,
                vec!["price_owned".into()],
            )
            .unwrap();
        let operation = Uuid::new_v4();
        let before = f.posts();
        assert!(matches!(
            f.service
                .create_bound_payment_link(&profile, operation, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.posts(), before);
        assert!(matches!(
            f.service
                .inspect_bound_payment_link_creation(operation)
                .await
                .unwrap(),
            Some(Outcome::Unresolved { .. })
        ));
    }
}

async fn hostile_reservation_input(f: &Fixture) {
    let counts = || async {
        sqlx::query_as::<_, (i64, i64)>("SELECT (SELECT count(*) FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1),(SELECT count(*) FROM fl_billing.payment_link_creation_audit WHERE tenant_id=$1)")
            .bind(f.tenant.id()).fetch_one(&f.owner).await.unwrap()
    };
    let before = counts().await;
    let posts = f.posts();
    let mut deep = Value::Null;
    for _ in 0..128 {
        deep = Value::Array(vec![deep]);
    }
    for payload in [
        deep,
        json!(vec![0; 10_000]),
        json!("x".repeat(70_000)),
        json!({"x".repeat(70_000): null}),
        json!("\u{0000}".repeat(12_000)),
        json!({"\u{0000}".repeat(12_000): null}),
    ] {
        let operation = Uuid::new_v4();
        assert!(
            f.store
                .reserve_payment_link_creation(&f.scope, operation, STRIPE_API_VERSION, &payload)
                .await
                .is_err()
        );
        assert!(
            f.store
                .inspect_payment_link_creation(operation)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(counts().await, before);
        assert_eq!(f.posts(), posts);
    }
}
