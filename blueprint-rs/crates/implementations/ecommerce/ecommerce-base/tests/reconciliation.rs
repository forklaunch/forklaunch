#[path = "support/payment_review.rs"]
mod payment_review;
use forklaunch_core::domain::AuditContext;
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    cancellation::{PaymentCancellationStore, VerifiedCancellation},
    reconciliation::{PaymentReconciliationStore, VerifiedCapture},
    store::InMemoryEcommerceStore,
};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentService, PaymentStatus,
};
use std::sync::Arc;
fn evidence() -> VerifiedCapture {
    VerifiedCapture {
        provider: "paypal".into(),
        provider_ref: "order-owned".into(),
        capture_id: "capture-owned".into(),
        amount_cents: 105,
        currency: "USD".into(),
    }
}
fn actor() -> AuditContext {
    AuditContext::actor("operator", "reconcile-owned")
}
async fn exercise<S: PaymentReconciliationStore + PaymentCancellationStore>(store: Arc<S>) {
    let service = BaseEcommerceService::new(store);
    let original = service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("payment".into()),
                order_id: "order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("order-owned".into()),
        )
        .await
        .unwrap();
    assert!(
        service
            .record_verified_capture(
                original.clone(),
                evidence(),
                AuditContext::system("anonymous")
            )
            .await
            .is_err()
    );
    let mut wrong = evidence();
    wrong.amount_cents = 106;
    assert!(
        service
            .record_verified_capture(original.clone(), wrong, actor())
            .await
            .is_err()
    );
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "order-owned".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        service
            .record_verified_capture(original, evidence(), actor())
            .await
            .unwrap_err()
            .code,
        "ecommerce_reconciliation_conflict"
    );
    let failed = service.get_payment("payment").await.unwrap();
    let repaired = service
        .record_verified_capture(failed, evidence(), actor())
        .await
        .unwrap();
    assert_eq!(repaired.status, PaymentStatus::Succeeded);
    // A delayed failure from the old capture attempt cannot reset verified completion.
    service
        .release_confirmation_claim("order-owned")
        .await
        .unwrap();
    assert_eq!(
        service.get_payment("payment").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert!(!service.claim_for_confirmation("order-owned").await.unwrap());
    exercise_cancellation(&service).await;
}
async fn exercise_cancellation<S: PaymentReconciliationStore + PaymentCancellationStore>(
    service: &BaseEcommerceService<S>,
) {
    let captured = service.get_payment("payment").await.unwrap();
    let cancellation = VerifiedCancellation {
        provider: "stripe".into(),
        provider_ref: "order-owned".into(),
        canceled_at: Some(1_700_000_000),
        amount_cents: 105,
        currency: "USD".into(),
    };
    assert_eq!(
        service
            .record_verified_cancellation(captured, cancellation.clone(), actor())
            .await
            .unwrap_err()
            .code,
        "ecommerce_terminal_payment_conflict"
    );
    let pending = service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("terminal".into()),
                order_id: "terminal-order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("terminal-ref".into()),
        )
        .await
        .unwrap();
    let cancellation = VerifiedCancellation {
        provider_ref: "terminal-ref".into(),
        ..cancellation
    };
    assert!(
        service
            .record_verified_cancellation(
                pending.clone(),
                cancellation.clone(),
                AuditContext::system("anonymous")
            )
            .await
            .is_err()
    );
    assert!(
        service
            .claim_for_confirmation("terminal-ref")
            .await
            .unwrap()
    );
    assert_eq!(
        service
            .record_verified_cancellation(pending, cancellation.clone(), actor())
            .await
            .unwrap_err()
            .code,
        "ecommerce_reconciliation_conflict"
    );
    let expected = service.get_payment("terminal").await.unwrap();
    let failed = service
        .record_verified_cancellation(expected, cancellation, actor())
        .await
        .unwrap();
    assert_eq!(failed.status, PaymentStatus::Failed);
    assert_eq!(failed.provider_ref.as_deref(), Some("terminal-ref"));
    service
        .release_confirmation_claim("terminal-ref")
        .await
        .unwrap();
    assert!(
        service
            .claim_for_confirmation("terminal-ref")
            .await
            .is_err()
    );
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "terminal-ref".into()
            })
            .await
            .is_err()
    );
    let proof = VerifiedCapture {
        provider_ref: "terminal-ref".into(),
        ..evidence()
    };
    assert_eq!(
        service
            .record_verified_capture(failed.clone(), proof, actor())
            .await
            .unwrap_err()
            .code,
        "ecommerce_terminal_payment_conflict"
    );
    assert_eq!(service.get_payment("terminal").await.unwrap(), failed);
}
async fn exercise_unknown_cancellation_time<
    S: PaymentReconciliationStore + PaymentCancellationStore,
>(
    service: &BaseEcommerceService<S>,
) {
    let expected = service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("void".into()),
                order_id: "void-order".into(),
                amount_cents: 105,
                currency: "USD".into(),
            },
            Some("void-ref".into()),
        )
        .await
        .unwrap();
    let evidence = VerifiedCancellation {
        provider: "paypal".into(),
        provider_ref: "void-ref".into(),
        canceled_at: None,
        amount_cents: 105,
        currency: "USD".into(),
    };
    let encoded = serde_json::to_value(&evidence).unwrap();
    assert!(encoded["canceled_at"].is_null());
    assert_eq!(
        serde_json::from_value::<VerifiedCancellation>(encoded).unwrap(),
        evidence
    );
    for timestamp in [0, -1] {
        assert!(
            service
                .record_verified_cancellation(
                    expected.clone(),
                    VerifiedCancellation {
                        canceled_at: Some(timestamp),
                        ..evidence.clone()
                    },
                    actor()
                )
                .await
                .is_err()
        );
    }
    let failed = service
        .record_verified_cancellation(expected, evidence, actor())
        .await
        .unwrap();
    assert_eq!(failed.status, PaymentStatus::Failed);
    assert!(service.claim_for_confirmation("void-ref").await.is_err());
}
#[tokio::test]
async fn memory_repairs_are_audited_and_reject_stale_snapshots() {
    let store = Arc::new(InMemoryEcommerceStore::default());
    exercise(Arc::clone(&store)).await;
    let audits = store.payment_reconciliation_audits().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].previous_status, PaymentStatus::Failed);
    assert_eq!(audits[0].actor, actor());
    assert_eq!(audits[0].evidence, evidence());
    let cancellations = store.payment_cancellation_audits().unwrap();
    assert_eq!(cancellations.len(), 1);
    assert_eq!(cancellations[0].previous_status, PaymentStatus::Succeeded);
    assert_eq!(cancellations[0].actor, actor());
    exercise_unknown_cancellation_time(&BaseEcommerceService::new(store.clone())).await;
    assert_eq!(
        store.payment_cancellation_audits().unwrap()[1]
            .evidence
            .canceled_at,
        None
    );
    payment_review::exercise(store.clone()).await;
}
#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
#[allow(clippy::too_many_lines)]
async fn postgres_repair_audit_rollback_tenants_and_restart() {
    use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
    use forklaunch_ecommerce_base::payment_review::CancelledOrderPaymentReviewStore;
    use forklaunch_ecommerce_base::postgres::PostgresEcommerceStore;
    use sqlx::postgres::PgPoolOptions;
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("ecom_reconcile_{suffix}");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records TO {role}; GRANT SELECT,INSERT ON fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.payment_cancellation_audit TO {role};")))
        .execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("fixture-only"),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(format!("reconcile-{suffix}")).unwrap();
    let other_tenant = TenantContext::new(format!("other-{suffix}")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("reconciliation-fixture-only").unwrap());
    let store = Arc::new(
        PostgresEcommerceStore::new(pool.clone(), tenant.clone(), Arc::clone(&keys), actor())
            .await
            .unwrap(),
    );
    exercise(Arc::clone(&store)).await;
    let service = BaseEcommerceService::new(Arc::clone(&store));
    let expected = service.get_payment("payment").await.unwrap();
    let other = PostgresEcommerceStore::new(
        pool.clone(),
        other_tenant.clone(),
        Arc::clone(&keys),
        actor(),
    )
    .await
    .unwrap();
    assert!(
        other
            .record_verified_capture(expected, evidence(), actor())
            .await
            .is_err()
    );
    // Exercise audit failure on a transition that would actually change state.
    let pending = service
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("rollback".into()),
                order_id: "order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("rollback-ref".into()),
        )
        .await
        .unwrap();
    let mut proof = evidence();
    proof.provider_ref = "rollback-ref".into();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.payment_reconciliation_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        service
            .record_verified_capture(pending.clone(), proof, actor())
            .await
            .is_err()
    );
    assert_eq!(service.get_payment("rollback").await.unwrap(), pending);
    let terminal = VerifiedCancellation {
        provider: "stripe".into(),
        provider_ref: "rollback-ref".into(),
        canceled_at: Some(1_700_000_000),
        amount_cents: 105,
        currency: "USD".into(),
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.payment_cancellation_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        service
            .record_verified_cancellation(pending.clone(), terminal.clone(), actor())
            .await
            .is_err()
    );
    assert_eq!(service.get_payment("rollback").await.unwrap(), pending);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.payment_cancellation_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        other
            .record_verified_cancellation(pending, terminal, actor())
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.payment_reconciliation_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let hidden: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_ecommerce.payment_reconciliation_audit")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hidden, 0);
    let hidden: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fl_ecommerce.payment_cancellation_audit")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(hidden, 0);
    let cancellation_rows: Vec<(String,String,i64,String)> = sqlx::query_as("SELECT payment_id,provider_ref,canceled_at,actor_id FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1")
        .bind(tenant.id()).fetch_all(&owner).await.unwrap();
    assert_eq!(
        cancellation_rows,
        vec![(
            "terminal".into(),
            "terminal-ref".into(),
            1_700_000_000,
            "operator".into()
        )]
    );
    let rows:Vec<(String,String,String,String)>=sqlx::query_as("SELECT payment_id,capture_id,actor_id,correlation_id FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1")
        .bind(tenant.id()).fetch_all(&owner).await.unwrap();
    assert_eq!(
        rows,
        vec![(
            "payment".into(),
            "capture-owned".into(),
            "operator".into(),
            "reconcile-owned".into()
        )]
    );
    exercise_unknown_cancellation_time(&service).await;
    let unknown_time: (Option<i64>, bool) = sqlx::query_as("SELECT canceled_at,reconciled_at IS NOT NULL FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1 AND payment_id='void'")
        .bind(tenant.id()).fetch_one(&owner).await.unwrap();
    assert_eq!(unknown_time, (None, true));
    let reopened = BaseEcommerceService::new(Arc::new(
        PostgresEcommerceStore::new(pool.clone(), tenant.clone(), keys, actor())
            .await
            .unwrap(),
    ));
    reopened
        .release_confirmation_claim("order-owned")
        .await
        .unwrap();
    assert_eq!(
        reopened.get_payment("payment").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert_eq!(
        reopened.get_payment("terminal").await.unwrap().status,
        PaymentStatus::Failed
    );
    assert!(
        reopened
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "terminal-ref".into()
            })
            .await
            .is_err()
    );
    assert_eq!(
        reopened.get_payment("void").await.unwrap().status,
        PaymentStatus::Failed
    );
    assert!(reopened.claim_for_confirmation("void-ref").await.is_err());
    payment_review::exercise(store.clone()).await;
    assert!(
        other
            .review_cancelled_order_payments("review-order", None, 10)
            .await
            .is_err()
    );
    drop(reopened);
    drop(other);
    drop(service);
    drop(store);
    pool.close().await;
    for table in [
        "records",
        "payment_reconciliation_audit",
        "payment_cancellation_audit",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=$1"
        )))
        .bind(tenant.id())
        .execute(&owner)
        .await
        .unwrap();
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role};DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
async fn cancellation_timestamp_migration_preserves_existing_provider_times() {
    let owner = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    // Exercise the exact old/new DDL in an isolated transactional schema. Rollback
    // removes all fixtures, including when the expected CHECK violation aborts it.
    let schema = format!("void_migration_{}", uuid::Uuid::new_v4().simple());
    let mut tx = owner.begin().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        include_str!("../migrations/7_payment_cancellation.sql").replace("fl_ecommerce", &schema),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let insert = format!(
        "INSERT INTO {schema}.payment_cancellation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,canceled_at,amount_cents,currency,actor_id,correlation_id) VALUES($1,'migration','payment','pending','stripe','reference',$2,105,'USD','operator','migration')"
    );
    sqlx::query(sqlx::AssertSqlSafe(insert.clone()))
        .bind(uuid::Uuid::new_v4())
        .bind(1_700_000_000_i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        include_str!("../migrations/8_optional_cancellation_time.sql")
            .replace("fl_ecommerce", &schema),
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let prior: Option<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT canceled_at FROM {schema}.payment_cancellation_audit"
    )))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(prior, Some(1_700_000_000));
    sqlx::query(sqlx::AssertSqlSafe(insert.clone()))
        .bind(uuid::Uuid::new_v4())
        .bind(Option::<i64>::None)
        .execute(&mut *tx)
        .await
        .unwrap();
    let invalid = sqlx::query(sqlx::AssertSqlSafe(insert.clone()))
        .bind(uuid::Uuid::new_v4())
        .bind(0_i64)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
    owner.close().await;
}
