#![cfg(feature = "postgres-identity")]
// Shared owned database lifecycle also supports the direct creation matrix.
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[allow(dead_code)]
#[path = "support/bound_creation_fixture.rs"]
mod fixture;
use fixture::{Fixture, request};
use forklaunch_billing_base::services::{BaseBillingService, BillingKind, BillingStore, Scope};
use forklaunch_billing_stripe::{
    bound_creation::BoundPaymentLinkCreationOutcome as Outcome, client::StripeBillingClient,
    managed_creation::ManagedBoundPaymentLinkCreationProfile, services::StripeBillingService,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
async fn control(base: &str, value: Value) -> Value {
    reqwest::Client::new()
        .post(format!("{base}/control"))
        .json(&value)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires actual companion protocol harness and isolated owned PostgreSQL"]
async fn managed_creation_and_inspect_only_publication() {
    let base = std::env::var("FORKLAUNCH_MANAGED_CREATION_BASE").unwrap();
    assert!(base.starts_with("http://127.0.0.1:"));
    let f = Fixture::new().await;
    let client =
        StripeBillingClient::managed(&base, "instance-owned".into(), "owned-instance-key".into())
            .unwrap();
    let service = Arc::new(StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        client,
    ));
    let profile = ManagedBoundPaymentLinkCreationProfile::trusted(
        f.scope.clone(),
        "managed-owned".into(),
        vec!["price_owned".into()],
    )
    .unwrap();
    successful(&f, &service, &profile, &base).await;
    interrupted(&f, &service, &profile, &base).await;
    rejected_receipts(&f, &service, &profile, &base).await;
    rollback(&f, &service, &profile, &base).await;
    ownership_and_expiry(&f, &service, &profile, &base).await;
    commit_acknowledgments(&f, &base).await;
    cancelled(&f, &service, &base).await;
    f.cleanup().await;
}
async fn successful(
    f: &Fixture,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &ManagedBoundPaymentLinkCreationProfile,
    base: &str,
) {
    let op = Uuid::new_v4();
    let Outcome::Completed { record, .. } = service
        .create_managed_bound_payment_link(profile, op, request())
        .await
        .unwrap()
    else {
        panic!("completed");
    };
    let id = Uuid::parse_str(&record.id).unwrap();
    assert!(
        record.provider_fields.as_ref().unwrap()["id"]
            .as_str()
            .unwrap()
            .starts_with("plink_managed_")
    );
    let binding = f
        .store
        .stripe_binding_by_public_id(&f.scope, BillingKind::PaymentLink, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.storage_id(), record.id);
    assert!(
        f.store
            .get(Scope::Cache, BillingKind::PaymentLink, &record.id)
            .await
            .unwrap()
            .is_none()
    );
    let calls = control(base, json!({})).await;
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, op, request())
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(control(base, json!({})).await["creates"], calls["creates"]);
    assert!(
        control(base, json!({})).await["lastIssuedAt"]
            .as_u64()
            .unwrap()
            > calls["lastIssuedAt"].as_u64().unwrap()
    );
    let mut changed = request();
    changed.amount += 1.0;
    assert!(
        service
            .create_managed_bound_payment_link(profile, op, changed)
            .await
            .is_err()
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
    let Outcome::Completed {
        record: receipt, ..
    } = service
        .reconcile_managed_bound_payment_link_creation(profile, op)
        .await
        .unwrap()
    else {
        panic!("receipt");
    };
    assert_eq!(receipt.provider_fields, record.provider_fields);
}
async fn interrupted(
    f: &Fixture,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &ManagedBoundPaymentLinkCreationProfile,
    base: &str,
) {
    control(base, json!({"scenario":"lost-delivery"})).await;
    let op = Uuid::new_v4();
    let before = f.counts().await;
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    let after = control(base, json!({"scenario":"ok"})).await;
    assert_eq!(f.counts().await, before);
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(control(base, json!({})).await["creates"], after["creates"]);
    control(base, json!({"scenario":"bad-current-config"})).await;
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, op)
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, before);
    control(base, json!({"scenario":"ok"})).await;
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, op)
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(control(base, json!({})).await["posts"], after["posts"]);
    control(base, json!({"scenario":"timeout"})).await;
    let pending = Uuid::new_v4();
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, pending, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    let calls = control(base, json!({"scenario":"ok"})).await;
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, pending)
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(control(base, json!({})).await["posts"], calls["posts"]);
}
async fn rejected_receipts(
    f: &Fixture,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &ManagedBoundPaymentLinkCreationProfile,
    base: &str,
) {
    for scenario in [
        "bad-envelope",
        "bad-provider",
        "rotate-config",
        "rotate-generation",
    ] {
        control(base, json!({"scenario":scenario})).await;
        let op = Uuid::new_v4();
        let before = f.counts().await;
        assert!(matches!(
            service
                .create_managed_bound_payment_link(profile, op, request())
                .await
                .unwrap(),
            Outcome::Unresolved { .. }
        ));
        assert_eq!(f.counts().await, before);
        let after = control(base, json!({"scenario":"ok"})).await;
        if scenario.starts_with("rotate") {
            assert!(matches!(
                service
                    .reconcile_managed_bound_payment_link_creation(profile, op)
                    .await
                    .unwrap(),
                Outcome::Unresolved { .. }
            ));
            assert_eq!(f.counts().await, before);
        } else {
            assert!(matches!(
                service
                    .reconcile_managed_bound_payment_link_creation(profile, op)
                    .await
                    .unwrap(),
                Outcome::Completed { .. }
            ));
        }
        assert_eq!(control(base, json!({})).await["posts"], after["posts"]);
    }
}
async fn rollback(
    f: &Fixture,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &ManagedBoundPaymentLinkCreationProfile,
    base: &str,
) {
    let op = Uuid::new_v4();
    let before = f.counts().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.provider_binding_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(f.counts().await, before);
    let calls = control(base, json!({})).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.provider_binding_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, op)
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(control(base, json!({})).await["posts"], calls["posts"]);
}
async fn cancelled(
    f: &Fixture,
    service: &Arc<
        StripeBillingService<forklaunch_billing_base::services::postgres::PostgresBillingStore>,
    >,
    base: &str,
) {
    control(base, json!({"scenario":"pause"})).await;
    let op = Uuid::new_v4();
    let service_copy = service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        let profile = ManagedBoundPaymentLinkCreationProfile::trusted(
            scope,
            "managed-owned".into(),
            vec!["price_owned".into()],
        )
        .unwrap();
        service_copy
            .create_managed_bound_payment_link(&profile, op, request())
            .await
    });
    for _ in 0..100 {
        if control(base, json!({})).await["entered"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(control(base, json!({})).await["entered"], true);
    let duplicate_profile = ManagedBoundPaymentLinkCreationProfile::trusted(
        f.scope.clone(),
        "managed-owned".into(),
        vec!["price_owned".into()],
    )
    .unwrap();
    let before = control(base, json!({})).await;
    assert!(matches!(
        service
            .create_managed_bound_payment_link(&duplicate_profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert_eq!(control(base, json!({})).await["creates"], before["creates"]);
    task.abort();
    let _ = task.await;
    let calls = control(base, json!({"resume":true,"scenario":"ok"})).await;
    let profile = ManagedBoundPaymentLinkCreationProfile::trusted(
        f.scope.clone(),
        "managed-owned".into(),
        vec!["price_owned".into()],
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(&profile, op)
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(control(base, json!({})).await["posts"], calls["posts"]);
}
async fn ownership_and_expiry(
    f: &Fixture,
    service: &StripeBillingService<
        forklaunch_billing_base::services::postgres::PostgresBillingStore,
    >,
    profile: &ManagedBoundPaymentLinkCreationProfile,
    base: &str,
) {
    use forklaunch_billing_base::services::postgres::StripeBindingScope;
    use forklaunch_core::domain::AuditContext;
    let direct_op = Uuid::new_v4();
    assert!(matches!(
        f.service
            .create_bound_payment_link(&f.profile, direct_op, request())
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, direct_op)
            .await
            .is_err()
    );
    assert!(
        f.service
            .create_managed_bound_payment_link(profile, Uuid::new_v4(), request())
            .await
            .is_err()
    );
    control(base, json!({"scenario":"lost-delivery"})).await;
    let op = Uuid::new_v4();
    assert!(matches!(
        service
            .create_managed_bound_payment_link(profile, op, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    control(base, json!({"scenario":"ok"})).await;
    let wrong_actor = f
        .store
        .with_audit_context(AuditContext::actor("other-actor", "managed-denial"))
        .unwrap();
    assert!(
        wrong_actor
            .inspect_managed_payment_link_publication(op)
            .await
            .is_err()
    );
    let wrong_scope = ManagedBoundPaymentLinkCreationProfile::trusted(
        StripeBindingScope::administrative("other-deployment".into(), "acct_owned".into(), false)
            .unwrap(),
        "managed-owned".into(),
        vec!["price_owned".into()],
    )
    .unwrap();
    assert!(
        service
            .reconcile_managed_bound_payment_link_creation(&wrong_scope, op)
            .await
            .is_err()
    );
    let changed_prices = ManagedBoundPaymentLinkCreationProfile::trusted(
        f.scope.clone(),
        "managed-owned".into(),
        vec!["price_owned".into(), "price_other".into()],
    )
    .unwrap();
    let calls = control(base, json!({})).await;
    let unchanged_records = f.counts().await;
    assert!(
        service
            .reconcile_managed_bound_payment_link_creation(&changed_prices, op)
            .await
            .is_err()
    );
    assert_eq!(control(base, json!({})).await["posts"], calls["posts"]);
    assert_eq!(f.counts().await, unchanged_records);
    let before = f.counts().await;
    let function = format!(
        "managed_wait_{}",
        &Uuid::new_v4().simple().to_string()[..16]
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fl_billing.{function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{}' THEN PERFORM pg_sleep(5.1); END IF; RETURN NEW; END; $$; CREATE TRIGGER {function} BEFORE INSERT ON fl_billing.records FOR EACH ROW EXECUTE FUNCTION fl_billing.{function}()",f.tenant.id()))).execute(&f.owner).await.unwrap();
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, op)
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
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(profile, op)
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
}
async fn commit_acknowledgments(f: &Fixture, base: &str) {
    use forklaunch_billing_base::services::postgres::PostgresBillingStore;
    use forklaunch_core::domain::AuditContext;
    use forklaunch_core::persistence::encryption::FieldEncryptor;
    use std::sync::atomic::Ordering;
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
            Arc::new(FieldEncryptor::new("owned-bound-create-encryption-key").unwrap()),
            AuditContext::actor("service:bound-create", "managed-lost-commit"),
        )
        .await
        .unwrap(),
    );
    let service = Arc::new(StripeBillingService::new(
        BaseBillingService::new(store),
        StripeBillingClient::managed(base, "instance-owned".into(), "owned-instance-key".into())
            .unwrap(),
    ));
    let profile = ManagedBoundPaymentLinkCreationProfile::trusted(
        f.scope.clone(),
        "managed-owned".into(),
        vec!["price_owned".into()],
    )
    .unwrap();
    let reserved = Uuid::new_v4();
    let before = control(base, json!({})).await;
    armed.store(true, Ordering::SeqCst);
    assert!(matches!(
        service
            .create_managed_bound_payment_link(&profile, reserved, request())
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    assert_eq!(control(base, json!({})).await["creates"], before["creates"]);
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(&profile, reserved)
            .await
            .unwrap(),
        Outcome::Unresolved { .. }
    ));
    control(base, json!({"scenario":"pause"})).await;
    let op = Uuid::new_v4();
    let copy = service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        let profile = ManagedBoundPaymentLinkCreationProfile::trusted(
            scope,
            "managed-owned".into(),
            vec!["price_owned".into()],
        )
        .unwrap();
        copy.create_managed_bound_payment_link(&profile, op, request())
            .await
    });
    for _ in 0..100 {
        if control(base, json!({})).await["entered"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(control(base, json!({})).await["entered"], true);
    armed.store(true, Ordering::SeqCst);
    let before = control(base, json!({"resume":true,"scenario":"ok"})).await;
    assert!(matches!(
        task.await.unwrap().unwrap(),
        Outcome::Unresolved { .. }
    ));
    assert!(!armed.load(Ordering::SeqCst));
    assert!(matches!(
        service
            .reconcile_managed_bound_payment_link_creation(&profile, op)
            .await
            .unwrap(),
        Outcome::Completed { .. }
    ));
    assert_eq!(control(base, json!({})).await["posts"], before["posts"]);
    pool.close().await;
    proxy.abort();
}
