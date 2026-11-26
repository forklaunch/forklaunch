#[cfg(feature = "postgres")]
#[path = "support/checkout_repair.rs"]
mod checkout_repair;
#[cfg(feature = "postgres")]
#[path = "support/provider_bindings.rs"]
mod provider_bindings;
#[cfg(feature = "postgres")]
#[path = "support/subscription_bindings.rs"]
mod subscription_bindings;
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope, memory::MemoryBillingStore,
};
use forklaunch_billing_interface::customers::CustomerService;
use forklaunch_billing_interface::services::{
    BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService,
    SubscriptionService, UpdatePlanDto,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
fn dto<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}
fn plan(id: &str, external: &str) -> Value {
    json!({"id":id,"name":"Pro","price":1999,"cadence":"month","currency":"USD","externalId":external,"billingProvider":"stripe","active":true,"providerFields":{"email":"private-billing@example.test"}})
}
fn checkout(id: &str) -> Value {
    json!({"id":id,"customerId":"customer","paymentMethods":["card"],"currency":"USD","status":"PENDING","expiresAt":"2020-01-01T00:00:00Z","uri":"https://checkout.example/private-session-token","providerFields":{"email":"private-billing@example.test"}})
}
fn value<T: Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}
async fn exercise_customers<S: BillingStore + 'static>(store: Arc<S>) {
    let service = BaseBillingService::new(store.clone());
    let request = json!({"id":"customer", "email":"private-customer@example.test", "name":"Private Customer"});
    let created = service.create_customer(dto(request.clone())).await.unwrap();
    assert_eq!(created.name.as_deref(), Some("Private Customer"));
    assert!(service.create_customer(dto(request)).await.is_err());
    let (email, name) = tokio::join!(
        service.update_customer(dto(
            json!({"id":"customer", "email":"updated-private-customer@example.test"})
        )),
        service.update_customer(dto(
            json!({"id":"customer", "name":"Updated Private Customer"})
        ))
    );
    email.unwrap();
    name.unwrap();
    let updated = service.get_customer("customer").await.unwrap();
    assert_eq!(updated.email, "updated-private-customer@example.test");
    assert_eq!(updated.name.as_deref(), Some("Updated Private Customer"));
    assert_eq!(updated.created_at, created.created_at);
    assert!(updated.updated_at >= created.updated_at);
    assert!(
        service
            .update_customer(dto(json!({"id":"customer", "email":""})))
            .await
            .is_err()
    );
    assert_eq!(
        service.get_customer("customer").await.unwrap().email,
        updated.email
    );
    service
        .update_customer(dto(json!({"id":"customer", "name":null})))
        .await
        .unwrap();
    assert!(
        service
            .get_customer("customer")
            .await
            .unwrap()
            .name
            .is_none()
    );
    assert_eq!(
        service
            .list_customers(Some(&["customer".into()]))
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(service.list_customers(Some(&[])).await.unwrap().is_empty());
    assert!(
        store
            .insert(
                Scope::Cache,
                BillingKind::Customer,
                value(created),
                Duration::from_secs(10)
            )
            .await
            .is_err()
    );
    service
        .create_customer(dto(
            json!({"id":"deleted-customer", "email":"deleted@example.test"}),
        ))
        .await
        .unwrap();
    service.delete_customer("deleted-customer").await.unwrap();
    assert!(service.get_customer("deleted-customer").await.is_err());
    let mut workers = Vec::new();
    for _ in 0..8 {
        let store = store.clone();
        workers.push(tokio::spawn(async move {
            BaseBillingService::new(store)
                .create_customer(dto(
                    json!({"id":"customer-race", "email":"race@example.test"}),
                ))
                .await
                .is_ok()
        }));
    }
    let mut winners = 0;
    for worker in workers {
        winners += usize::from(worker.await.unwrap());
    }
    assert_eq!(winners, 1);
}
#[allow(clippy::too_many_lines, clippy::float_cmp)] // Exact integer-valued money round trips.
async fn exercise<S: BillingStore + 'static>(store: Arc<S>) {
    exercise_customers(Arc::clone(&store)).await;
    let service = BaseBillingService::new(Arc::clone(&store));
    service
        .create_plan(dto(plan("plan", "price")))
        .await
        .unwrap();
    let first = BaseBillingService::new(Arc::clone(&store));
    let second = BaseBillingService::new(Arc::clone(&store));
    let (a, b) = tokio::join!(
        first.update_plan(dto(
            json!({"id":"plan","description":"concurrent-description"})
        )),
        second.update_plan(dto(json!({"id":"plan","features":["concurrent-feature"]})))
    );
    a.unwrap();
    b.unwrap();
    let current = service.get_plan("plan").await.unwrap();
    assert_eq!(
        current.description.as_deref(),
        Some("concurrent-description")
    );
    assert_eq!(current.features.unwrap(), vec!["concurrent-feature"]);
    let mut invalid: UpdatePlanDto = dto(json!({"id":"plan"}));
    invalid.price = Some(f64::NAN);
    assert!(service.update_plan(invalid).await.is_err());
    assert_eq!(service.get_plan("plan").await.unwrap().price, 1999.0);
    let mut tasks = Vec::new();
    for index in 0..8 {
        let store = Arc::clone(&store);
        tasks.push(tokio::spawn(async move {
            BaseBillingService::new(store)
                .create_plan(dto(plan(&format!("race-{index}"), "unique-provider-price")))
                .await
                .is_ok()
        }));
    }
    let mut winners = 0;
    for task in tasks {
        winners += usize::from(task.await.unwrap());
    }
    assert_eq!(winners, 1);
    let subscription = json!({"id":"subscription","partyId":"party","partyType":"USER","productId":"plan","active":true,"externalId":"provider-subscription","startDate":"2026-01-01T00:00:00Z","status":"TRIALING","billingProvider":"stripe","providerFields":{"email":"private-billing@example.test"}});
    service
        .create_subscription(dto(subscription))
        .await
        .unwrap();
    assert_eq!(
        service.get_user_subscription("party").await.unwrap().id,
        "subscription"
    );
    assert!(
        service
            .get_organization_subscription("party")
            .await
            .is_err()
    );
    service.cancel_subscription("subscription").await.unwrap();
    assert!(service.get_user_subscription("party").await.is_err());
    service.resume_subscription("subscription").await.unwrap();
    assert_eq!(
        service
            .find_subscription_id_by_external_id("provider-subscription")
            .await
            .unwrap()
            .as_deref(),
        Some("subscription")
    );
    service
        .create_checkout_session(dto(checkout("cached")))
        .await
        .unwrap();
    assert!(
        service.get_checkout_session("cached").await.is_ok(),
        "Provider expiry must not replace configured cache TTL"
    );
    assert!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "cached")
            .await
            .unwrap()
            .is_none()
    );
    let mut replacement = checkout("cached");
    replacement["customerId"] = json!("replacement-customer");
    service
        .create_checkout_session(dto(replacement))
        .await
        .unwrap();
    assert_eq!(
        service
            .get_checkout_session("cached")
            .await
            .unwrap()
            .customer_id,
        "replacement-customer"
    );
    service.expire_checkout_session("cached").await.unwrap();
    assert!(service.get_checkout_session("cached").await.is_err());
    assert!(service.expire_checkout_session("cached").await.is_err());
    let backed = BaseBillingService::new(Arc::clone(&store)).with_database_backup(true);
    backed
        .create_checkout_session(dto(checkout("backed")))
        .await
        .unwrap();
    backed.handle_checkout_success("backed").await.unwrap();
    assert!(backed.get_checkout_session("backed").await.is_err());
    assert_eq!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "backed")
            .await
            .unwrap()
            .unwrap()["status"],
        "SUCCESS"
    );
    let link = json!({"id":"link","amount":100,"paymentMethods":["card"],"currency":"USD","status":"PENDING"});
    backed.create_payment_link(dto(link)).await.unwrap();
    backed
        .update_payment_link(dto(json!({"id":"link","amount":0})))
        .await
        .unwrap();
    assert_eq!(backed.get_payment_link("link").await.unwrap().amount, 0.0);
    assert_eq!(
        backed
            .list_payment_links(Some(&["link".to_owned(), "link".to_owned()]))
            .await
            .unwrap()
            .len(),
        2
    );
    backed.handle_payment_success("link").await.unwrap();
    assert!(
        store
            .get(Scope::Durable, BillingKind::PaymentLink, "link")
            .await
            .unwrap()
            .is_none()
    );
    assert!(backed.handle_payment_success("link").await.is_err());
    backed.create_billing_portal_session(dto(json!({"id":"portal","customerId":"customer","expiresAt":"2020-01-01T00:00:00Z","uri":"https://portal.example/private-session-token","providerFields":null}))).await.unwrap();
    let updated = backed
        .update_billing_portal_session(dto(
            json!({"id":"portal","uri":"https://portal.example/updated-session-token"}),
        ))
        .await
        .unwrap();
    assert_eq!(updated.customer_id, "customer");
    assert_eq!(value(updated)["providerFields"], Value::Null);
    backed
        .expire_billing_portal_session("portal")
        .await
        .unwrap();
    assert!(
        store
            .get(Scope::Durable, BillingKind::BillingPortal, "portal")
            .await
            .unwrap()
            .is_some()
    );
    let short = BaseBillingService::new(Arc::clone(&store))
        .with_database_backup(true)
        .with_cache_ttl(Duration::from_millis(200))
        .unwrap();
    short
        .create_checkout_session(dto(checkout("ttl")))
        .await
        .unwrap();
    assert!(short.get_checkout_session("ttl").await.is_ok());
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(short.get_checkout_session("ttl").await.is_err());
    assert!(
        store
            .get(Scope::Durable, BillingKind::CheckoutSession, "ttl")
            .await
            .unwrap()
            .is_some()
    );
}
#[tokio::test]
async fn memory_billing_lifecycle_cache_validation_and_concurrency() {
    exercise(Arc::new(MemoryBillingStore::new())).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires an isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
#[allow(clippy::too_many_lines)]
async fn postgres_billing_rls_encryption_audit_concurrency_and_reopening() {
    use forklaunch_billing_base::services::postgres::{PostgresBillingStore, WebhookResolution};
    use forklaunch_core::domain::AuditContext;
    use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
    use sqlx::postgres::PgPoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresBillingStore::migrate(&owner).await.unwrap();
    let role = format!("fl_bill_{}", uuid::Uuid::new_v4().simple());
    let password = "billing-fixture-only";
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD '{password}'; GRANT USAGE ON SCHEMA fl_billing TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.records TO {role}; GRANT INSERT ON fl_billing.sensitive_read_audit TO {role}; GRANT SELECT,INSERT ON fl_billing.webhook_receipts TO {role};GRANT SELECT,INSERT,DELETE ON fl_billing.webhook_attempts TO {role};GRANT INSERT ON fl_billing.webhook_reconciliation_audit TO {role}; GRANT SELECT,INSERT,DELETE ON fl_billing.platform_event_nonces TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password(password),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(format!("{role}-a")).unwrap();
    let other_tenant = TenantContext::new(format!("{role}-b")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("billing-old-fixture-secret").unwrap());
    let audit = AuditContext::actor("service:billing-fixture", "billing-fixture-correlation");
    assert!(
        PostgresBillingStore::new(
            owner.clone(),
            tenant.clone(),
            Arc::clone(&keys),
            audit.clone()
        )
        .await
        .is_err()
    );
    let store = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            tenant.clone(),
            Arc::clone(&keys),
            audit.clone(),
        )
        .await
        .unwrap(),
    );
    exercise(Arc::clone(&store)).await;
    let other = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            other_tenant.clone(),
            Arc::clone(&keys),
            audit.clone(),
        )
        .await
        .unwrap(),
    );
    let effects = AtomicUsize::new(0);
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_once", "unknown", || async {
                effects.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap()
    );
    assert!(
        !store
            .with_webhook_receipt("stripe", "evt_once", "unknown", || async {
                panic!("Duplicate delivery must not apply effects")
            })
            .await
            .unwrap()
    );
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_once", "other", || async { Ok(()) })
            .await
            .is_err()
    );
    assert!(
        other
            .with_webhook_receipt("stripe", "evt_once", "unknown", || async { Ok(()) })
            .await
            .unwrap()
    );
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_retry", "unknown", || async {
                Err(forklaunch_core::domain::BlueprintError::new(
                    "fixture", "failed",
                ))
            })
            .await
            .is_err()
    );
    assert_eq!(
        store
            .with_webhook_receipt("stripe", "evt_retry", "unknown", || async {
                panic!("Ambiguous effects must never replay automatically")
            })
            .await
            .unwrap_err()
            .code,
        "billing_webhook_reconciliation_required"
    );
    let pending = store.pending_webhooks(100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(other.pending_webhooks(100).await.unwrap().is_empty());
    assert!(
        other
            .resolve_webhook(&pending[0], WebhookResolution::Applied)
            .await
            .is_err()
    );
    let anonymous = store
        .with_audit_context(AuditContext {
            actor_id: None,
            correlation_id: "anonymous".into(),
        })
        .unwrap();
    assert!(
        anonymous
            .resolve_webhook(&pending[0], WebhookResolution::Applied)
            .await
            .is_err()
    );
    store
        .resolve_webhook(&pending[0], WebhookResolution::RetryAllowed)
        .await
        .unwrap();
    assert!(
        store
            .resolve_webhook(&pending[0], WebhookResolution::Applied)
            .await
            .is_err()
    );
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_retry", "unknown", || async { Ok(()) })
            .await
            .unwrap()
    );
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let concurrent_store = Arc::clone(&store);
    let processing = tokio::spawn(async move {
        concurrent_store
            .with_webhook_receipt("stripe", "evt_busy", "unknown", || async {
                entered.send(()).unwrap();
                resume.await.unwrap();
                Ok(())
            })
            .await
    });
    ready.await.unwrap();
    assert_eq!(
        store
            .with_webhook_receipt("stripe", "evt_busy", "unknown", || async {
                panic!("Concurrent delivery must not apply effects")
            })
            .await
            .unwrap_err()
            .code,
        "billing_webhook_busy"
    );
    release.send(()).unwrap();
    assert!(processing.await.unwrap().unwrap());
    // Cancellation after committed local effects survives store reconstruction.
    let (entered, ready) = tokio::sync::oneshot::channel();
    let interrupted_store = Arc::clone(&store);
    let interrupted = tokio::spawn(async move {
        interrupted_store
            .with_webhook_receipt("stripe", "evt_interrupted", "unknown", || async {
                interrupted_store
                    .claim_platform_event_nonce("committed-effect")
                    .await?;
                entered.send(()).unwrap();
                std::future::pending::<()>().await;
                Ok(())
            })
            .await
    });
    ready.await.unwrap();
    let pending = store.pending_webhooks(100).await.unwrap().pop().unwrap();
    assert_eq!(
        store
            .resolve_webhook(&pending, WebhookResolution::Applied)
            .await
            .unwrap_err()
            .code,
        "billing_webhook_busy"
    );
    interrupted.abort();
    assert!(interrupted.await.unwrap_err().is_cancelled());
    let restarted = PostgresBillingStore::new(
        pool.clone(),
        tenant.clone(),
        Arc::clone(&keys),
        audit.clone(),
    )
    .await
    .unwrap();
    // Wait only for the aborted transaction's asynchronous rollback to release its lock.
    let mut blocked = None;
    for _ in 0..100 {
        let error = restarted
            .with_webhook_receipt("stripe", "evt_interrupted", "unknown", || async {
                panic!("Committed effects must survive interruption without replay")
            })
            .await
            .unwrap_err();
        if error.code != "billing_webhook_busy" {
            blocked = Some(error.code);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        blocked.as_deref(),
        Some("billing_webhook_reconciliation_required")
    );
    assert!(
        !restarted
            .claim_platform_event_nonce("committed-effect")
            .await
            .unwrap()
    );
    restarted
        .resolve_webhook(&pending, WebhookResolution::Applied)
        .await
        .unwrap();
    assert!(
        !restarted
            .with_webhook_receipt("stripe", "evt_interrupted", "unknown", || async {
                panic!("Reconciled effects must remain deduplicated")
            })
            .await
            .unwrap()
    );
    assert!(restarted.pending_webhooks(100).await.unwrap().is_empty());
    assert!(restarted.pending_webhooks(0).await.is_err());
    let resolutions: Vec<(String,String,String)> = sqlx::query_as(
        "SELECT resolution,actor_id,correlation_id FROM fl_billing.webhook_reconciliation_audit WHERE tenant_id=$1 ORDER BY resolved_at")
        .bind(tenant.id()).fetch_all(&owner).await.unwrap();
    assert_eq!(resolutions.len(), 2);
    assert_eq!(resolutions[0].0, "retry_allowed");
    assert_eq!(resolutions[1].0, "applied");
    assert!(
        resolutions
            .iter()
            .all(|(_, actor, correlation)| actor == "service:billing-fixture"
                && correlation == "billing-fixture-correlation")
    );
    // A stale inspection cannot resolve a later attempt of the same event.
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_stale", "unknown", || async {
                Err(forklaunch_core::domain::BlueprintError::new(
                    "fixture", "failed",
                ))
            })
            .await
            .is_err()
    );
    let stale = store.pending_webhooks(100).await.unwrap().pop().unwrap();
    store
        .resolve_webhook(&stale, WebhookResolution::RetryAllowed)
        .await
        .unwrap();
    assert!(
        store
            .with_webhook_receipt("stripe", "evt_stale", "unknown", || async {
                Err(forklaunch_core::domain::BlueprintError::new(
                    "fixture", "failed",
                ))
            })
            .await
            .is_err()
    );
    let current = store.pending_webhooks(100).await.unwrap().pop().unwrap();
    assert_ne!(current.attempt_id, stale.attempt_id);
    assert!(
        store
            .resolve_webhook(&stale, WebhookResolution::Applied)
            .await
            .is_err()
    );
    // Failed audit insertion must neither erase the attempt nor create a receipt.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.webhook_reconciliation_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        store
            .resolve_webhook(&current, WebhookResolution::Applied)
            .await
            .is_err()
    );
    assert_eq!(
        store.pending_webhooks(100).await.unwrap(),
        vec![current.clone()]
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.webhook_reconciliation_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    store
        .resolve_webhook(&current, WebhookResolution::Applied)
        .await
        .unwrap();
    let hidden: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_billing.webhook_attempts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(hidden, 0);
    drop(restarted);
    drop(anonymous);
    assert!(store.claim_platform_event_nonce("nonce").await.unwrap());
    assert!(!store.claim_platform_event_nonce("nonce").await.unwrap());
    assert!(other.claim_platform_event_nonce("nonce").await.unwrap());
    let limited_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password(password),
        )
        .await
        .unwrap();
    let limited = Arc::new(
        PostgresBillingStore::new(
            limited_pool.clone(),
            tenant.clone(),
            Arc::clone(&keys),
            audit.clone(),
        )
        .await
        .unwrap(),
    );
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let worker_store = Arc::clone(&limited);
    let worker = tokio::spawn(async move {
        worker_store
            .with_webhook_receipt("stripe", "evt_capacity", "unknown", || async {
                // Receipt holds one connection; a business read still has another.
                worker_store
                    .get(Scope::Durable, BillingKind::Plan, "plan")
                    .await?;
                entered.send(()).unwrap();
                resume.await.unwrap();
                Ok(())
            })
            .await
    });
    ready.await.unwrap();
    assert_eq!(
        limited
            .with_webhook_receipt("stripe", "evt_other", "unknown", || async { Ok(()) })
            .await
            .unwrap_err()
            .code,
        "billing_webhook_busy"
    );
    release.send(()).unwrap();
    assert!(worker.await.unwrap().unwrap());
    assert!(
        limited
            .with_webhook_receipt("stripe", "evt_other", "unknown", || async { Ok(()) })
            .await
            .unwrap()
    );
    drop(limited);
    limited_pool.close().await;
    let other_service = BaseBillingService::new(Arc::clone(&other));
    assert!(other_service.get_plan("plan").await.is_err());
    assert!(other_service.get_customer("customer").await.is_err());
    assert!(other_service.get_user_subscription("party").await.is_err());
    assert!(
        other_service
            .find_subscription_id_by_external_id("provider-subscription")
            .await
            .unwrap()
            .is_none()
    );
    other_service
        .create_plan(dto(plan("plan", "price")))
        .await
        .unwrap();
    let (unscoped,): (i64,) = sqlx::query_as("SELECT count(*) FROM fl_billing.records")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unscoped, 0);
    let data: Vec<(Value,)> =
        sqlx::query_as("SELECT data FROM fl_billing.records WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_all(&owner)
            .await
            .unwrap();
    for (row,) in data {
        let raw = row.to_string();
        assert!(!raw.contains("private-billing@example.test"));
        assert!(!raw.contains("private-customer@example.test"));
        assert!(!raw.contains("Private Customer"));
        assert!(!raw.contains("private-session-token"));
        assert!(!raw.contains("updated-session-token"));
    }
    let (count_before,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert!(count_before > 0);
    let service = BaseBillingService::new(Arc::clone(&store));
    assert_eq!(
        service
            .find_subscription_id_by_external_id("provider-subscription")
            .await
            .unwrap()
            .as_deref(),
        Some("subscription")
    );
    let (count_after,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1")
            .bind(tenant.id())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(
        count_before, count_after,
        "Provider-ID lookup must not decrypt provider data"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.sensitive_read_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(service.get_plan("plan").await.is_err());
    assert!(service.get_customer("customer").await.is_err());
    assert!(
        service
            .update_customer(dto(
                json!({"id":"customer", "email":"must-rollback@example.test"})
            ))
            .await
            .is_err()
    );
    assert!(
        service
            .update_plan(dto(json!({"id":"plan","description":"must-rollback"})))
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.sensitive_read_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(
        service
            .get_plan("plan")
            .await
            .unwrap()
            .description
            .as_deref(),
        Some("concurrent-description")
    );
    let rotated = Arc::new(
        FieldEncryptor::new("billing-new-fixture-secret")
            .unwrap()
            .with_previous_keys(&["billing-old-fixture-secret".to_owned()])
            .unwrap(),
    );
    let reopened = BaseBillingService::new(Arc::new(
        PostgresBillingStore::new(pool.clone(), tenant.clone(), rotated, audit.clone())
            .await
            .unwrap(),
    ));
    assert_eq!(
        reopened
            .get_plan("plan")
            .await
            .unwrap()
            .provider_fields
            .unwrap()["email"],
        "private-billing@example.test"
    );
    reopened
        .update_plan(dto(json!({"id":"plan","description":"rotated"})))
        .await
        .unwrap();
    let new_only = BaseBillingService::new(Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            tenant.clone(),
            Arc::new(FieldEncryptor::new("billing-new-fixture-secret").unwrap()),
            audit.clone(),
        )
        .await
        .unwrap(),
    ));
    assert_eq!(
        new_only
            .get_plan("plan")
            .await
            .unwrap()
            .description
            .as_deref(),
        Some("rotated")
    );
    // Authenticated envelopes bind scope, kind, record and field, preventing
    // an owner/storage bug from substituting another record's encrypted payload.
    assert_eq!(
        reopened.get_customer("customer").await.unwrap().email,
        "updated-private-customer@example.test"
    );
    reopened
        .update_customer(dto(json!({"id":"customer", "name":"Rotated Customer"})))
        .await
        .unwrap();
    assert_eq!(
        new_only
            .get_customer("customer")
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("Rotated Customer")
    );
    let customer_cipher: Value = sqlx::query_scalar("SELECT data->'email' FROM fl_billing.records WHERE tenant_id=$1 AND kind='customer' AND id='customer'")
        .bind(tenant.id()).fetch_one(&owner).await.unwrap();
    let other_customer_cipher: Value = sqlx::query_scalar("SELECT data->'email' FROM fl_billing.records WHERE tenant_id=$1 AND kind='customer' AND id='customer-race'")
        .bind(tenant.id()).fetch_one(&owner).await.unwrap();
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{email}',$2) WHERE tenant_id=$1 AND kind='customer' AND id='customer'")
        .bind(tenant.id()).bind(other_customer_cipher).execute(&owner).await.unwrap();
    assert!(reopened.get_customer("customer").await.is_err());
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{email}',$2) WHERE tenant_id=$1 AND kind='customer' AND id='customer'")
        .bind(tenant.id()).bind(customer_cipher).execute(&owner).await.unwrap();
    assert!(new_only.get_customer("customer").await.is_ok());
    let (original,): (Value,) = sqlx::query_as("SELECT data->'providerFields' FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='plan' AND id='plan'")
        .bind(tenant.id()).fetch_one(&owner).await.unwrap();
    let (other_cipher,): (Value,) = sqlx::query_as("SELECT data->'providerFields' FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='subscription' AND id='subscription'")
        .bind(tenant.id()).fetch_one(&owner).await.unwrap();
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{providerFields}',$2) WHERE tenant_id=$1 AND scope='durable' AND kind='plan' AND id='plan'")
        .bind(tenant.id()).bind(other_cipher).execute(&owner).await.unwrap();
    assert!(reopened.get_plan("plan").await.is_err());
    sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,'{providerFields}',$2) WHERE tenant_id=$1 AND scope='durable' AND kind='plan' AND id='plan'")
        .bind(tenant.id()).bind(original).execute(&owner).await.unwrap();
    assert!(new_only.get_plan("plan").await.is_ok());
    assert!(store.purge_expired_cache().await.unwrap() >= 1);
    let actors: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT actor_id,correlation_id FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1",
    )
    .bind(tenant.id())
    .fetch_all(&owner)
    .await
    .unwrap();
    assert!(actors.iter().all(|(actor, correlation)| actor.as_deref()
        == Some("service:billing-fixture")
        && correlation == "billing-fixture-correlation"));
    checkout_repair::exercise(store.clone(), &other, &owner, &role, tenant.id()).await;
    assert!(
        !reopened
            .store()
            .with_webhook_receipt(
                "stripe",
                "evt_repair-owned",
                "checkout.session.completed",
                || async { panic!("reopened replica must observe repair receipt") }
            )
            .await
            .unwrap()
    );
    provider_bindings::exercise(
        &store,
        &other,
        reopened.store(),
        &owner,
        &pool,
        &role,
        tenant.id(),
    )
    .await;
    subscription_bindings::exercise(
        &store,
        &other,
        reopened.store(),
        &owner,
        &pool,
        &role,
        tenant.id(),
    )
    .await;
    drop(new_only);
    drop(reopened);
    drop(service);
    drop(store);
    drop(other_service);
    drop(other);
    pool.close().await;
    for table in [
        "provider_bindings",
        "provider_binding_audit",
        "records",
        "sensitive_read_audit",
        "webhook_receipts",
        "webhook_attempts",
        "webhook_reconciliation_audit",
        "platform_event_nonces",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_billing.{table} WHERE tenant_id=ANY($1)"
        )))
        .bind(vec![tenant.id(), other_tenant.id()])
        .execute(&owner)
        .await
        .unwrap();
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
