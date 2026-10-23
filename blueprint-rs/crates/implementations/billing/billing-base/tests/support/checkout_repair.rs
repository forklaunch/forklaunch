use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope,
    postgres::{PendingWebhook, PostgresBillingStore},
};
use forklaunch_billing_interface::services::CheckoutSessionService;
use forklaunch_core::domain::BlueprintError;
use serde_json::json;
use std::{sync::Arc, time::Duration};

async fn prepare(
    store: &Arc<PostgresBillingStore>,
    id: &str,
    backup: bool,
    expired: bool,
) -> PendingWebhook {
    let service = BaseBillingService::new(store.clone()).with_database_backup(backup);
    let mut value = super::checkout(id);
    value["providerFields"] = json!({"id":id,"object":"checkout.session","customer":"customer","client_secret":"secret-not-for-audit"});
    service
        .create_checkout_session(super::dto(value))
        .await
        .unwrap();
    let event = format!("evt_{id}");
    let kind = if expired {
        "checkout.session.expired"
    } else {
        "checkout.session.completed"
    };
    assert_eq!(
        store
            .with_webhook_receipt("stripe", &event, kind, || async {
                Err(BlueprintError::new(
                    "fixture",
                    "interrupted after provider metadata update",
                ))
            })
            .await
            .unwrap_err()
            .code,
        "fixture"
    );
    store
        .pending_webhooks(1000)
        .await
        .unwrap()
        .into_iter()
        .find(|pending| pending.event_id == event)
        .unwrap()
}
#[allow(clippy::too_many_lines)]
pub async fn exercise(
    store: Arc<PostgresBillingStore>,
    other: &PostgresBillingStore,
    owner: &sqlx::PgPool,
    role: &str,
    tenant: &str,
) {
    let pending = prepare(&store, "repair-owned", true, false).await;
    assert!(
        store
            .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", false)
            .await
            .is_err(),
        "changed backup configuration needs explicit review"
    );
    // Local customerId and stored provider identity must both agree with the
    // independently retrieved provider customer, not merely with each other.
    store
        .patch(
            Scope::Cache,
            BillingKind::CheckoutSession,
            "repair-owned",
            json!({"customerId":"other-local-customer"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    assert!(
        store
            .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
            .await
            .is_err()
    );
    store
        .patch(
            Scope::Cache,
            BillingKind::CheckoutSession,
            "repair-owned",
            json!({"customerId":"customer"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();

    assert!(
        other
            .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
            .await
            .is_err()
    );
    assert!(
        store
            .snapshot_verified_checkout_repair(&pending, "repair-owned", "wrong-customer", true)
            .await
            .is_err()
    );
    assert!(
        store
            .snapshot_verified_checkout_repair(&pending, "missing-session", "customer", false)
            .await
            .is_err()
    );
    let snapshot = store
        .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
        .await
        .unwrap();
    store
        .patch(
            Scope::Cache,
            BillingKind::CheckoutSession,
            "repair-owned",
            json!({"status":"changed"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    assert!(
        store
            .complete_verified_checkout_repair(snapshot)
            .await
            .is_err()
    );
    store
        .patch(
            Scope::Cache,
            BillingKind::CheckoutSession,
            "repair-owned",
            json!({"status":"PENDING"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    // Fail AFTER the backup update and cache removal: all local changes, receipt
    // and attempt deletion must roll back with the denied actor-audit insertion.
    let snapshot = store
        .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.webhook_reconciliation_audit FROM {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(
        store
            .complete_verified_checkout_repair(snapshot)
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.webhook_reconciliation_audit TO {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert_eq!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "repair-owned")
            .await
            .unwrap()
            .unwrap()["status"],
        "PENDING"
    );
    assert!(
        store
            .get(Scope::Cache, BillingKind::CheckoutSession, "repair-owned")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .pending_webhooks(1000)
            .await
            .unwrap()
            .iter()
            .any(|item| item.attempt_id == pending.attempt_id)
    );
    // Fail after the success receipt INSERT, on pending-attempt DELETE. Even
    // the newly inserted receipt/audit must disappear with the local writes.
    let snapshot = store
        .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE DELETE ON fl_billing.webhook_attempts FROM {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    assert!(
        store
            .complete_verified_checkout_repair(snapshot)
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT DELETE ON fl_billing.webhook_attempts TO {role}"
    )))
    .execute(owner)
    .await
    .unwrap();
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_billing.webhook_receipts WHERE tenant_id=$1 AND event_id=$2",
    )
    .bind(tenant)
    .bind(&pending.event_id)
    .fetch_one(owner)
    .await
    .unwrap();
    assert_eq!(receipts, 0);
    assert_eq!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "repair-owned")
            .await
            .unwrap()
            .unwrap()["status"],
        "PENDING"
    );
    let first = store
        .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
        .await
        .unwrap();
    let second = store
        .snapshot_verified_checkout_repair(&pending, "repair-owned", "customer", true)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.complete_verified_checkout_repair(first),
        store.complete_verified_checkout_repair(second)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "repair-owned")
            .await
            .unwrap()
            .unwrap()["status"],
        "SUCCESS"
    );
    assert!(
        store
            .get(Scope::Cache, BillingKind::CheckoutSession, "repair-owned")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !store
            .with_webhook_receipt("stripe", &pending.event_id, &pending.event_type, || async {
                panic!("repaired event must not repeat provider effects")
            })
            .await
            .unwrap()
    );
    let audit:(i64,bool)=sqlx::query_as("SELECT count(*),bool_and(resolution='applied' AND actor_id IS NOT NULL) FROM fl_billing.webhook_reconciliation_audit WHERE tenant_id=$1 AND event_id=$2").bind(tenant).bind(&pending.event_id).fetch_one(owner).await.unwrap();
    assert_eq!(audit, (1, true));
    // Backup disabled: existing cached ownership permits local-only cleanup.
    let pending = prepare(&store, "repair-expired", false, true).await;
    let snapshot = store
        .snapshot_verified_checkout_repair(&pending, "repair-expired", "customer", false)
        .await
        .unwrap();
    store
        .complete_verified_checkout_repair(snapshot)
        .await
        .unwrap();
    assert!(
        store
            .get(Scope::Cache, BillingKind::CheckoutSession, "repair-expired")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .get(
                Scope::Durable,
                BillingKind::CheckoutSession,
                "repair-expired"
            )
            .await
            .unwrap()
            .is_none()
    );
    // A crash after backup completion but before cache deletion is also repairable.
    let pending = prepare(&store, "repair-partial", true, false).await;
    store
        .patch(
            Scope::Durable,
            BillingKind::CheckoutSession,
            "repair-partial",
            json!({"status":"SUCCESS"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    let snapshot = store
        .snapshot_verified_checkout_repair(&pending, "repair-partial", "customer", true)
        .await
        .unwrap();
    store
        .complete_verified_checkout_repair(snapshot)
        .await
        .unwrap();
    assert!(
        store
            .get(Scope::Cache, BillingKind::CheckoutSession, "repair-partial")
            .await
            .unwrap()
            .is_none()
    );
    // Conflicting final local state stays unresolved; never overwrite failure.
    let pending = prepare(&store, "repair-contradiction", true, false).await;
    store
        .patch(
            Scope::Durable,
            BillingKind::CheckoutSession,
            "repair-contradiction",
            json!({"status":"FAILED"}),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    assert!(
        store
            .snapshot_verified_checkout_repair(&pending, "repair-contradiction", "customer", true)
            .await
            .is_err()
    );
    let pending = prepare(&store, "repair-retry", false, false).await;
    let stale = store
        .snapshot_verified_checkout_repair(&pending, "repair-retry", "customer", false)
        .await
        .unwrap();
    store
        .resolve_webhook(
            &pending,
            forklaunch_billing_base::services::postgres::WebhookResolution::RetryAllowed,
        )
        .await
        .unwrap();
    assert!(
        store
            .with_webhook_receipt("stripe", &pending.event_id, &pending.event_type, || async {
                Err(BlueprintError::new("fixture", "new attempt interrupted"))
            })
            .await
            .is_err()
    );
    assert!(
        store
            .complete_verified_checkout_repair(stale)
            .await
            .is_err()
    );
    let retry = store
        .pending_webhooks(1000)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.event_id == pending.event_id)
        .unwrap();
    assert_ne!(retry.attempt_id, pending.attempt_id);
    assert!(
        store
            .get(Scope::Cache, BillingKind::CheckoutSession, "repair-retry")
            .await
            .unwrap()
            .is_some()
    );
}
