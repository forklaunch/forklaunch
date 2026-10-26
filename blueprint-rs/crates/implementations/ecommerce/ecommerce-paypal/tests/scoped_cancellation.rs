#![cfg(feature = "postgres")]
#[path = "../../../billing/billing-stripe/tests/support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    cancellation::VerifiedCancellation,
    postgres::{PaymentProviderScope, PostgresEcommerceStore, ScopedPaypalCancellationOutcome},
};
use forklaunch_ecommerce_interface::{CreatePaymentDto, PaymentDto, PaymentService, PaymentStatus};
use forklaunch_ecommerce_paypal::{PaypalClient, PaypalPaymentService, context::PaypalEnvironment};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
#[derive(Clone)]
struct Fixture {
    body: Arc<Mutex<Value>>,
    change: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    owner: PgPool,
    tenant: String,
    store: Arc<PostgresEcommerceStore>,
    scope: PaymentProviderScope,
    payment: PaymentDto,
}
async fn provider(State(s): State<Fixture>, headers: HeaderMap) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-secret");
    assert!(!headers.contains_key("paypal-auth-assertion"));
    assert!(!headers.contains_key("paypal-partner-attribution-id"));
    s.calls.fetch_add(1, Ordering::SeqCst);
    match s.change.load(Ordering::SeqCst) {
        1 => {
            sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='order'").bind(&s.tenant).execute(&s.owner).await.unwrap();
        }
        2 => {
            let binding = s
                .store
                .payment_provider_binding(&s.scope, &s.payment.id)
                .await
                .unwrap();
            s.store
                .revoke_payment_provider_binding(&binding)
                .await
                .unwrap();
            s.store
                .provision_payment_provider_binding(&s.scope, &s.payment)
                .await
                .unwrap();
        }
        3 => {
            sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"ORDER_replaced\"') WHERE tenant_id=$1 AND kind='payment'").bind(&s.tenant).execute(&s.owner).await.unwrap();
        }
        4 => {
            let mut tx = s.owner.begin().await.unwrap();
            sqlx::query("SELECT id FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' FOR UPDATE").bind(&s.tenant).fetch_all(&mut *tx).await.unwrap();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(5100)).await;
                tx.rollback().await.unwrap();
            });
        }
        5 => {
            sqlx::query(
                "UPDATE fl_ecommerce.records SET data=data WHERE tenant_id=$1 AND kind='order'",
            )
            .bind(&s.tenant)
            .execute(&s.owner)
            .await
            .unwrap();
        }
        6 => {
            sqlx::query("UPDATE fl_ecommerce.payment_provider_bindings SET merchant_id='Q7C9N6S4YRQXX' WHERE tenant_id=$1").bind(&s.tenant).execute(&s.owner).await.unwrap();
        }
        7 => {
            sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"failed\"') WHERE tenant_id=$1 AND kind='payment'").bind(&s.tenant).execute(&s.owner).await.unwrap();
        }
        _ => {}
    }
    Json(s.body.lock().unwrap().clone())
}
fn actor() -> AuditContext {
    AuditContext::actor("operator", "scoped-cancel-fixture")
}
fn evidence() -> VerifiedCancellation {
    VerifiedCancellation {
        provider: "paypal".into(),
        provider_ref: "ORDER_owned".into(),
        canceled_at: None,
        amount_cents: 105,
        currency: "USD".into(),
    }
}
async fn audit_count(owner: &PgPool, tenant: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(owner)
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn paypal_scoped_cancellation_atomic_receipts_scope_cas_and_deadline() {
    let owner = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("paypal_cancel_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit,fl_ecommerce.payment_cancellation_audit,fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.scoped_paypal_cancellation_receipts TO {role}"))).execute(&owner).await.unwrap();
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
    let keys = Arc::new(FieldEncryptor::new("owned-scoped-cancel-key").unwrap());
    let store = Arc::new(
        PostgresEcommerceStore::new(
            pool.clone(),
            TenantContext::new(role.clone()).unwrap(),
            keys.clone(),
            actor(),
        )
        .await
        .unwrap(),
    );
    sqlx::query("INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,'order','order','{\"id\":\"order\",\"status\":\"cancelled\",\"shippingAddress\":\"undecryptable-not-selected\"}')").bind(&role).execute(&owner).await.unwrap();
    let base = BaseEcommerceService::new(store.clone());
    let payment = base
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("payment".into()),
                order_id: "order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("ORDER_owned".into()),
        )
        .await
        .unwrap();
    let scope = PaymentProviderScope::administrative(
        "paypal".into(),
        "owned".into(),
        "Q7C9N6S4YRQXG".into(),
        false,
    )
    .unwrap();
    store
        .provision_payment_provider_binding(&scope, &payment)
        .await
        .unwrap();
    // Primary Orders schema: terminal VOIDED plus explicit comprehensive empty histories.
    let terminal = json!({"id":"ORDER_owned","intent":"CAPTURE","status":"VOIDED","purchase_units":[{"reference_id":"order","payee":{"merchant_id":"Q7C9N6S4YRQXG"},"amount":{"currency_code":"USD","value":"1.05"},"payments":{"captures":[],"authorizations":[],"refunds":[]}}]});
    let state = Fixture {
        body: Arc::new(Mutex::new(terminal.clone())),
        change: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
        owner: owner.clone(),
        tenant: role.clone(),
        store: store.clone(),
        scope: scope.clone(),
        payment: payment.clone(),
    };
    let app = Router::new()
        .route(
            "/v1/oauth2/token",
            post(|State(s): State<Fixture>, headers: HeaderMap, body: String| async move {
                s.calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(
                    headers["authorization"],
                    "Basic b3duZWQtY2xpZW50Om93bmVkLXNlY3JldA=="
                );
                assert_eq!(body, "grant_type=client_credentials");
                Json(json!({"access_token":"owned-secret","token_type":"Bearer","expires_in":300}))
            }),
        )
        .route("/v2/checkout/orders/ORDER_owned", get(provider))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(2), 4096)
        .unwrap();
    let client = PaypalClient::with_trusted_environment_transport(
        "owned-client".into(),
        "owned-secret".into(),
        PaypalEnvironment::Sandbox,
        http.clone(),
    )
    .unwrap();
    let service = PaypalPaymentService::new(base, client);
    let unprofiled = PaypalPaymentService::new(
        BaseEcommerceService::new(store.clone()),
        PaypalClient::with_transport("owned-client".into(), "owned-secret".into(), http).unwrap(),
    );
    assert!(
        unprofiled
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    let wrong_env = PaymentProviderScope::administrative(
        "paypal".into(),
        "owned".into(),
        "Q7C9N6S4YRQXG".into(),
        true,
    )
    .unwrap();
    assert!(
        service
            .reconcile_bound_voided_order_payment(&wrong_env, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert!(
        service
            .reconcile_bound_voided_order_payment(
                &scope,
                "payment",
                AuditContext::actor(" ", "x"),
                Uuid::new_v4()
            )
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), 0);
    let mut invalid = vec![];
    for (key, value) in [
        ("status", json!("COMPLETED")),
        ("status", json!("APPROVED")),
        ("intent", json!("AUTHORIZE")),
        ("id", json!("other")),
    ] {
        let mut bad = terminal.clone();
        bad[key] = value;
        invalid.push(bad);
    }
    for key in ["captures", "authorizations", "refunds"] {
        for value in [
            Value::Null,
            json!([{"id":"prior"}]),
            json!({}),
            json!(false),
        ] {
            let mut bad = terminal.clone();
            bad["purchase_units"][0]["payments"][key] = value;
            invalid.push(bad);
        }
        let mut bad = terminal.clone();
        bad["purchase_units"][0]["payments"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        invalid.push(bad);
    }
    for (key, value) in [
        ("reference_id", json!("other")),
        ("payee", json!({"merchant_id":"Q7C9N6S4YRQXX"})),
        ("amount", json!({"currency_code":"USD","value":"1.06"})),
        ("amount", json!({"currency_code":"EUR","value":"1.05"})),
    ] {
        let mut bad = terminal.clone();
        bad["purchase_units"][0][key] = value;
        invalid.push(bad);
    }
    for bad in invalid {
        *state.body.lock().unwrap() = bad;
        assert!(
            service
                .reconcile_bound_voided_order_payment(&scope, "payment", actor(), Uuid::new_v4())
                .await
                .is_err()
        );
        assert_eq!(audit_count(&owner, &role).await, 0);
    }
    *state.body.lock().unwrap() = terminal;
    for change in [1, 2, 3, 4, 5, 6, 7] {
        state.change.store(change, Ordering::SeqCst);
        let id = Uuid::new_v4();
        assert!(
            service
                .reconcile_bound_voided_order_payment(&scope, "payment", actor(), id)
                .await
                .is_err()
        );
        assert!(
            store
                .scoped_paypal_cancellation_receipt(id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(audit_count(&owner, &role).await, 0);
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"cancelled\"') WHERE tenant_id=$1 AND kind='order'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"ORDER_owned\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.payment_provider_bindings SET merchant_id='Q7C9N6S4YRQXG' WHERE tenant_id=$1").bind(&role).execute(&owner).await.unwrap();
    }
    state.change.store(0, Ordering::SeqCst);
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    // A current local succeeded record cannot be rewritten, even with no captured audit.
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"succeeded\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    let before = state.calls.load(Ordering::SeqCst);
    assert!(
        service
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), before);
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    sqlx::query("INSERT INTO fl_ecommerce.payment_reconciliation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,capture_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,'payment','pending','paypal','ORDER_owned','ch_prior',105,'USD','operator','fixture')").bind(Uuid::new_v4().to_string()).bind(&role).execute(&owner).await.unwrap();
    assert!(
        service
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    // Receipt INSERT denial occurs after terminal audit INSERT; both must roll back immediately.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.scoped_paypal_cancellation_receipts FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let denied_id = Uuid::new_v4();
    assert!(
        service
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), denied_id)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_paypal_cancellation_receipt(denied_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.scoped_paypal_cancellation_receipts TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    // Expiry after the writes but before COMMIT rolls back audit, receipt and status.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = store
        .cancelled_paypal_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    let stale_id = Uuid::new_v4();
    assert!(
        store
            .record_scoped_paypal_order_cancellation(
                snapshot,
                evidence(),
                actor(),
                stale_id,
                || {
                    if checks.fetch_add(1, Ordering::SeqCst) >= 2 {
                        Err(BlueprintError::new("expired", "Expired"))
                    } else {
                        Ok(())
                    }
                }
            )
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_paypal_cancellation_receipt(stale_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    // End-to-end valid provider read publishes Failed, preserving order/ref.
    let valid_id = Uuid::new_v4();
    let valid = service
        .reconcile_bound_voided_order_payment(&scope, "payment", actor(), valid_id)
        .await
        .unwrap();
    match valid {
        ScopedPaypalCancellationOutcome::Recorded { receipt, payment } => {
            assert_eq!(payment.status, PaymentStatus::Failed);
            assert_eq!(payment.provider_ref.as_deref(), Some("ORDER_owned"));
            assert_eq!(
                store
                    .scoped_paypal_cancellation_receipt(valid_id)
                    .await
                    .unwrap(),
                Some(receipt)
            );
        }
        ScopedPaypalCancellationOutcome::RecordedEvidenceNoLongerCurrent { .. } => {
            panic!("fresh fixture expected")
        }
    }
    let order_status: String = sqlx::query_scalar(
        "SELECT data->>'status' FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order'",
    )
    .bind(&role)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(order_status, "cancelled");
    reset(&owner, &role).await;
    // Simulate deadline expiry only after COMMIT: return a written receipt, not rollback.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = store
        .cancelled_paypal_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    let expired_id = Uuid::new_v4();
    let result = store
        .record_scoped_paypal_order_cancellation(snapshot, evidence(), actor(), expired_id, || {
            if checks.fetch_add(1, Ordering::SeqCst) >= 3 {
                Err(BlueprintError::new("expired", "Expired"))
            } else {
                Ok(())
            }
        })
        .await
        .unwrap();
    assert!(matches!(
        result,
        ScopedPaypalCancellationOutcome::RecordedEvidenceNoLongerCurrent { .. }
    ));
    assert!(
        store
            .scoped_paypal_cancellation_receipt(expired_id)
            .await
            .unwrap()
            .is_some()
    );
    let receipt_id = expired_id;
    assert_eq!(audit_count(&owner, &role).await, 1);
    // Reusing a receipt UUID rolls back the attempted second terminal audit.
    assert!(
        service
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), receipt_id)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    // Receipt is tenant isolated and API/DB append-only despite broad runtime grants.
    let other = PostgresEcommerceStore::new(
        pool.clone(),
        TenantContext::new(format!("other-{role}")).unwrap(),
        keys,
        actor(),
    )
    .await
    .unwrap();
    assert!(
        other
            .scoped_paypal_cancellation_receipt(receipt_id)
            .await
            .unwrap()
            .is_none()
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(&role)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("DELETE FROM fl_ecommerce.scoped_paypal_cancellation_receipts")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
    // A foreign tenant cannot snapshot another tenant's opaque binding.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    assert!(
        other
            .cancelled_paypal_order_payment_snapshot(&binding)
            .await
            .is_err()
    );
    // Scoped one-publication constraint applies even with a new receipt UUID.
    assert!(
        service
            .reconcile_bound_voided_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    reset(&owner, &role).await;
    // Concurrent snapshots cannot publish twice, even if the payment began Failed.
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"failed\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let a = store
        .cancelled_paypal_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let b = store
        .cancelled_paypal_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.record_scoped_paypal_order_cancellation(
            a,
            evidence(),
            actor(),
            Uuid::new_v4(),
            || Ok(())
        ),
        store.record_scoped_paypal_order_cancellation(
            b,
            evidence(),
            actor(),
            Uuid::new_v4(),
            || Ok(())
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(audit_count(&owner, &role).await, 1);
    reset(&owner, &role).await;
    // The COMMIT reaches PostgreSQL; only its acknowledgement is discarded.
    let options = owner.connect_options();
    let (port, armed, proxy) =
        commit_ack_proxy::start(options.get_host().into(), options.get_port()).await;
    let proxied = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            (*pool.connect_options())
                .clone()
                .host("127.0.0.1")
                .port(port)
                .ssl_mode(sqlx::postgres::PgSslMode::Disable),
        )
        .await
        .unwrap();
    let uncertain = PostgresEcommerceStore::new(
        proxied.clone(),
        TenantContext::new(role.clone()).unwrap(),
        Arc::new(FieldEncryptor::new("owned-scoped-cancel-key").unwrap()),
        actor(),
    )
    .await
    .unwrap();
    let binding = uncertain
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = uncertain
        .cancelled_paypal_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let unknown = Uuid::new_v4();
    armed.store(true, Ordering::SeqCst);
    let outcome = uncertain
        .record_scoped_paypal_order_cancellation(snapshot, evidence(), actor(), unknown, || Ok(()))
        .await;
    assert_eq!(
        outcome.err().unwrap().code,
        "ecommerce_paypal_cancellation_commit_unknown"
    );
    assert!(
        store
            .scoped_paypal_cancellation_receipt(unknown)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    assert_eq!(
        service
            .base()
            .get_payment("payment")
            .await
            .unwrap()
            .provider_ref
            .as_deref(),
        Some("ORDER_owned")
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Failed
    );
    let provider_timestamp: Option<i64> = sqlx::query_scalar(
        "SELECT canceled_at FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1",
    )
    .bind(&role)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert!(provider_timestamp.is_none());
    proxied.close().await;
    proxy.abort();
    server.abort();
    pool.close().await;
    for table in [
        "scoped_paypal_cancellation_receipts",
        "payment_cancellation_audit",
        "payment_reconciliation_audit",
        "payment_provider_binding_audit",
        "records",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=$1"
        )))
        .bind(&role)
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
}

async fn reset(owner: &PgPool, tenant: &str) {
    for table in [
        "scoped_paypal_cancellation_receipts",
        "payment_cancellation_audit",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_ecommerce.{table} WHERE tenant_id=$1"
        )))
        .bind(tenant)
        .execute(owner)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='payment'").bind(tenant).execute(owner).await.unwrap();
}
