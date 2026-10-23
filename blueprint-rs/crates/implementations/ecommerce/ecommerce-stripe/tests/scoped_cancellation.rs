#![cfg(feature = "postgres")]
use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    cancellation::VerifiedCancellation,
    postgres::{PaymentProviderScope, PostgresEcommerceStore, ScopedCancellationOutcome},
};
use forklaunch_ecommerce_interface::{CreatePaymentDto, PaymentDto, PaymentService, PaymentStatus};
use forklaunch_ecommerce_stripe::{StripeClient, StripeConnect, StripePaymentService};
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
    connected: bool,
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
    assert_eq!(
        headers.get("stripe-account").map(|v| v.to_str().unwrap()),
        s.connected.then_some("acct_owned")
    );
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
            sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"pi_replaced\"') WHERE tenant_id=$1 AND kind='payment'").bind(&s.tenant).execute(&s.owner).await.unwrap();
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
            sqlx::query("UPDATE fl_ecommerce.payment_provider_bindings SET merchant_id='acct_substituted' WHERE tenant_id=$1").bind(&s.tenant).execute(&s.owner).await.unwrap();
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
        provider: "stripe".into(),
        provider_ref: "pi_owned".into(),
        canceled_at: Some(1_700_000_000),
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
async fn direct_scoped_cancellation_atomic_receipts_scope_cas_and_deadline() {
    run_profile(false).await;
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn connected_scoped_cancellation_atomic_receipts_scope_cas_and_deadline() {
    run_profile(true).await;
}
#[allow(clippy::too_many_lines)]
async fn run_profile(connected: bool) {
    let owner = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("ecom_cancel_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit,fl_ecommerce.payment_cancellation_audit,fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.scoped_payment_cancellation_receipts TO {role}"))).execute(&owner).await.unwrap();
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
            Some("pi_owned".into()),
        )
        .await
        .unwrap();
    let scope = PaymentProviderScope::administrative(
        "stripe".into(),
        "owned".into(),
        "acct_owned".into(),
        false,
    )
    .unwrap();
    store
        .provision_payment_provider_binding(&scope, &payment)
        .await
        .unwrap();
    // Narrow terminal fixture follows Stripe's cancel response shape; no POST is made.
    let terminal = json!({"id":"pi_owned","object":"payment_intent","livemode":false,"amount":105,"amount_received":0,"amount_capturable":0,"currency":"usd","metadata":{"orderId":"order"},"status":"canceled","canceled_at":1_700_000_000,"latest_charge":null});
    let state = Fixture {
        body: Arc::new(Mutex::new(terminal.clone())),
        connected,
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
            "/v1/account",
            get(|State(s): State<Fixture>, headers: HeaderMap| async move {
                assert_eq!(
                    headers.get("stripe-account").map(|v| v.to_str().unwrap()),
                    s.connected.then_some("acct_owned")
                );
                Json(json!({"object":"account","id":"acct_owned"}))
            }),
        )
        .route(
            "/v1/balance",
            get(|State(s): State<Fixture>, headers: HeaderMap| async move {
                assert_eq!(
                    headers.get("stripe-account").map(|v| v.to_str().unwrap()),
                    s.connected.then_some("acct_owned")
                );
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route("/v1/payment_intents/pi_owned", get(provider))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeClient::with_transport(
        "owned-secret".into(),
        ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(2), 4096)
            .unwrap(),
    )
    .unwrap();
    let service = Profile::new(base, client, connected);
    if connected {
        assert!(
            service
                .service
                .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), Uuid::new_v4())
                .await
                .is_err()
        );
        let wrong_scope = PaymentProviderScope::administrative(
            "stripe".into(),
            "owned".into(),
            "acct_other".into(),
            false,
        )
        .unwrap();
        assert!(
            service
                .reconcile_bound_cancelled_order_payment(
                    &wrong_scope,
                    "payment",
                    actor(),
                    Uuid::new_v4()
                )
                .await
                .is_err()
        );
    } else {
        assert!(
            service
                .service
                .reconcile_bound_connected_cancelled_order_payment(
                    &scope,
                    "payment",
                    actor(),
                    Uuid::new_v4()
                )
                .await
                .is_err()
        );
    }
    assert_eq!(state.calls.load(Ordering::SeqCst), 0);
    for (key, value) in [
        ("status", json!("processing")),
        ("amount_received", json!(105)),
        ("amount_capturable", json!(105)),
        ("latest_charge", json!("ch_prior")),
        (
            "latest_charge",
            json!({"object":"charge","status":"failed"}),
        ),
        ("canceled_at", Value::Null),
        ("livemode", json!(true)),
        ("metadata", json!({"orderId":"other"})),
    ] {
        let mut bad = terminal.clone();
        bad[key] = value;
        *state.body.lock().unwrap() = bad;
        assert!(
            service
                .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), Uuid::new_v4())
                .await
                .is_err()
        );
        assert_eq!(audit_count(&owner, &role).await, 0);
    }
    let mut absent = terminal.clone();
    absent.as_object_mut().unwrap().remove("latest_charge");
    *state.body.lock().unwrap() = absent;
    assert!(
        service
            .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    *state.body.lock().unwrap() = terminal;
    for change in [1, 2, 3, 4, 5, 6] {
        state.change.store(change, Ordering::SeqCst);
        let id = Uuid::new_v4();
        assert!(
            service
                .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), id)
                .await
                .is_err()
        );
        assert!(
            store
                .scoped_cancellation_receipt(id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(audit_count(&owner, &role).await, 0);
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"cancelled\"') WHERE tenant_id=$1 AND kind='order'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"pi_owned\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.payment_provider_bindings SET merchant_id='acct_owned' WHERE tenant_id=$1").bind(&role).execute(&owner).await.unwrap();
    }
    state.change.store(0, Ordering::SeqCst);
    // A current local succeeded record cannot be rewritten, even with no captured audit.
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"succeeded\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    let before = state.calls.load(Ordering::SeqCst);
    assert!(
        service
            .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), before);
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    sqlx::query("INSERT INTO fl_ecommerce.payment_reconciliation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,capture_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,'payment','pending','stripe','pi_owned','ch_prior',105,'USD','operator','fixture')").bind(Uuid::new_v4().to_string()).bind(&role).execute(&owner).await.unwrap();
    assert!(
        service
            .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), Uuid::new_v4())
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
        "REVOKE INSERT ON fl_ecommerce.scoped_payment_cancellation_receipts FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let denied_id = Uuid::new_v4();
    assert!(
        service
            .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), denied_id)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_cancellation_receipt(denied_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.scoped_payment_cancellation_receipts TO {role}"
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
        .cancelled_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    let stale_id = Uuid::new_v4();
    assert!(
        store
            .record_scoped_cancelled_order_cancellation(
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
            .scoped_cancellation_receipt(stale_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    let receipt_id = Uuid::new_v4();
    let outcome = service
        .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), receipt_id)
        .await
        .unwrap();
    let receipt = match outcome {
        ScopedCancellationOutcome::Recorded { receipt, payment } => {
            assert_eq!(payment.status, PaymentStatus::Failed);
            assert_eq!(payment.provider_ref.as_deref(), Some("pi_owned"));
            receipt
        }
        ScopedCancellationOutcome::RecordedEvidenceNoLongerCurrent { .. } => {
            panic!("fresh receipt expected")
        }
    };
    assert_eq!(
        store.scoped_cancellation_receipt(receipt_id).await.unwrap(),
        Some(receipt)
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    // Reusing a receipt UUID rolls back the attempted second terminal audit.
    assert!(
        service
            .reconcile_bound_cancelled_order_payment(&scope, "payment", actor(), receipt_id)
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
            .scoped_cancellation_receipt(receipt_id)
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
        sqlx::query("DELETE FROM fl_ecommerce.scoped_payment_cancellation_receipts")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
    // Simulate deadline expiry only after COMMIT: return a written receipt, not rollback.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = store
        .cancelled_order_payment_snapshot(&binding)
        .await
        .unwrap();
    let checks = AtomicUsize::new(0);
    let expired_id = Uuid::new_v4();
    let result = store
        .record_scoped_cancelled_order_cancellation(
            snapshot,
            evidence(),
            actor(),
            expired_id,
            || {
                if checks.fetch_add(1, Ordering::SeqCst) >= 3 {
                    Err(BlueprintError::new("expired", "Expired"))
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        result,
        ScopedCancellationOutcome::RecordedEvidenceNoLongerCurrent { .. }
    ));
    assert!(
        store
            .scoped_cancellation_receipt(expired_id)
            .await
            .unwrap()
            .is_some()
    );
    server.abort();
    pool.close().await;
    for table in [
        "scoped_payment_cancellation_receipts",
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

struct Profile {
    service: StripePaymentService<PostgresEcommerceStore>,
    connected: bool,
}
impl Profile {
    fn new(
        base: BaseEcommerceService<PostgresEcommerceStore>,
        client: StripeClient,
        connected: bool,
    ) -> Self {
        let mut service = StripePaymentService::new(base, client);
        if connected {
            service = service.with_connect(StripeConnect::new("acct_owned".into(), 0).unwrap());
        }
        Self { service, connected }
    }
    async fn reconcile_bound_cancelled_order_payment(
        &self,
        scope: &PaymentProviderScope,
        id: &str,
        actor: AuditContext,
        receipt: Uuid,
    ) -> forklaunch_core::domain::BlueprintResult<ScopedCancellationOutcome> {
        if self.connected {
            self.service
                .reconcile_bound_connected_cancelled_order_payment(scope, id, actor, receipt)
                .await
        } else {
            self.service
                .reconcile_bound_cancelled_order_payment(scope, id, actor, receipt)
                .await
        }
    }
    fn base(&self) -> &BaseEcommerceService<PostgresEcommerceStore> {
        self.service.base()
    }
}
