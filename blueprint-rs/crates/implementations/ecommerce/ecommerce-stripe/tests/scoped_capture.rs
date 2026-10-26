#![cfg(feature = "postgres")]
use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    http::HeaderMap,
    routing::get,
};
#[path = "../../../billing/billing-stripe/tests/support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    postgres::{PaymentProviderScope, PostgresEcommerceStore, ScopedCaptureOutcome},
    reconciliation::VerifiedCapture,
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
    context_calls: Arc<AtomicUsize>,
    owner: PgPool,
    tenant: String,
    store: Arc<PostgresEcommerceStore>,
    scope: PaymentProviderScope,
    payment: PaymentDto,
}
async fn provider(
    State(s): State<Fixture>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-secret");
    assert_eq!(
        headers.get("stripe-account").map(|v| v.to_str().unwrap()),
        s.connected.then_some("acct_owned")
    );
    assert_eq!(
        uri.query(),
        Some("expand%5B%5D=latest_charge.balance_transaction")
    );
    s.calls.fetch_add(1, Ordering::SeqCst);
    match s.change.load(Ordering::SeqCst) {
        1 => {
            sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"cancelled\"') WHERE tenant_id=$1 AND kind='order'").bind(&s.tenant).execute(&s.owner).await.unwrap();
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
    AuditContext::actor("operator", "scoped-capture-fixture")
}
fn evidence() -> VerifiedCapture {
    VerifiedCapture {
        provider: "stripe".into(),
        provider_ref: "pi_owned".into(),
        capture_id: "ch_owned".into(),
        amount_cents: 105,
        currency: "USD".into(),
    }
}
async fn audit_count(owner: &PgPool, tenant: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(owner)
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn direct_scoped_capture_atomic_receipts_scope_cas_and_deadline() {
    Box::pin(run_profile(false)).await;
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn connected_scoped_capture_atomic_receipts_scope_cas_and_deadline() {
    Box::pin(run_profile(true)).await;
}
#[allow(clippy::too_many_lines)]
async fn run_profile(connected: bool) {
    let owner = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("ecom_capture_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit,fl_ecommerce.payment_cancellation_audit,fl_ecommerce.payment_reconciliation_audit,fl_ecommerce.scoped_payment_capture_receipts TO {role}"))).execute(&owner).await.unwrap();
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
    let keys = Arc::new(FieldEncryptor::new("owned-scoped-capture-key").unwrap());
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
    sqlx::query("INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,'order','order','{\"id\":\"order\",\"status\":\"pending\",\"shippingAddress\":\"undecryptable-not-selected\"}')").bind(&role).execute(&owner).await.unwrap();
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
    let mut terminal = json!({"id":"pi_owned","object":"payment_intent","livemode":false,"status":"succeeded","amount":105,"amount_received":105,"amount_capturable":0,"currency":"usd","metadata":{"orderId":"order"},"latest_charge":{"id":"ch_owned","object":"charge","livemode":false,"payment_intent":"pi_owned","status":"succeeded","paid":true,"captured":true,"refunded":false,"disputed":false,"amount":105,"amount_captured":105,"amount_refunded":0,"currency":"usd","balance_transaction":{"id":"txn_owned","object":"balance_transaction","source":"ch_owned","amount":105,"currency":"usd","type":"charge","status":"pending","exchange_rate":null}}});
    if connected {
        terminal["on_behalf_of"] = Value::Null;
        terminal["application_fee_amount"] = json!(5);
        let charge = &mut terminal["latest_charge"];
        charge["on_behalf_of"] = Value::Null;
        charge["source_transfer"] = Value::Null;
        charge["transfer_data"] = Value::Null;
        charge["application_fee_amount"] = json!(5);
        charge["application_fee"] = json!("fee_historical");
        charge["balance_transaction"]["fee"] = json!(8);
        charge["balance_transaction"]["net"] = json!(97);
    }
    let state = Fixture {
        body: Arc::new(Mutex::new(terminal.clone())),
        connected,
        change: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
        context_calls: Arc::new(AtomicUsize::new(0)),
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
                s.context_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers["authorization"], "Bearer owned-secret");
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
                s.context_calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers["authorization"], "Bearer owned-secret");
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
    let direct = StripePaymentService::new(base, client.clone());
    let connected_service =
        StripePaymentService::new(BaseEcommerceService::new(store.clone()), client)
            .with_connect(StripeConnect::new("acct_owned".into(), 0).unwrap());
    assert!(
        connected_service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert!(
        direct
            .reconcile_bound_connected_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(state.context_calls.load(Ordering::SeqCst), 0);
    assert_eq!(state.calls.load(Ordering::SeqCst), 0);
    let service = Profile {
        service: if connected { connected_service } else { direct },
        connected,
    };
    if connected {
        let wrong_scope = PaymentProviderScope::administrative(
            "stripe".into(),
            "owned".into(),
            "acct_other".into(),
            false,
        )
        .unwrap();
        assert!(
            service
                .reconcile_bound_captured_payment(&wrong_scope, "payment", actor(), Uuid::new_v4())
                .await
                .is_err()
        );
        assert_eq!(state.context_calls.load(Ordering::SeqCst), 0);
        assert_eq!(state.calls.load(Ordering::SeqCst), 0);
        for (pointer, value) in [
            ("/on_behalf_of", json!("acct_other")),
            ("/transfer_data", json!({"destination":"acct_other"})),
            ("/latest_charge/source_transfer", json!("tr_incoming")),
            (
                "/latest_charge/transfer_data",
                json!({"destination":"acct_other"}),
            ),
            ("/latest_charge/transfer", json!("tr_outgoing")),
            ("/latest_charge/application_fee_amount", json!(6)),
            ("/application_fee_amount", json!(106)),
            (
                "/latest_charge/application_fee",
                json!({"id":"fee_historical"}),
            ),
        ] {
            let mut bad = terminal.clone();
            if pointer == "/transfer_data" {
                bad["transfer_data"] = value;
            } else if pointer == "/latest_charge/transfer" {
                bad["latest_charge"]["transfer"] = value;
            } else {
                *bad.pointer_mut(pointer).unwrap() = value;
            }
            *state.body.lock().unwrap() = bad;
            assert!(
                service
                    .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
                    .await
                    .is_err()
            );
            assert_eq!(audit_count(&owner, &role).await, 0);
        }
        for (parent, key) in [
            ("", "on_behalf_of"),
            ("/latest_charge", "source_transfer"),
            ("/latest_charge", "transfer_data"),
            ("/latest_charge", "on_behalf_of"),
            ("", "application_fee_amount"),
        ] {
            let mut bad = terminal.clone();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            *state.body.lock().unwrap() = bad;
            assert!(
                service
                    .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
                    .await
                    .is_err()
            );
        }
    } else {
        assert_eq!(state.calls.load(Ordering::SeqCst), 0);
    }
    for (key, value) in [
        ("amount_received", json!(104)),
        ("livemode", json!(true)),
        ("amount_capturable", json!(105)),
        ("latest_charge", Value::Null),
    ] {
        let mut bad = terminal.clone();
        bad[key] = value;
        *state.body.lock().unwrap() = bad;
        assert!(
            service
                .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
                .await
                .is_err()
        );
        assert_eq!(audit_count(&owner, &role).await, 0);
    }
    *state.body.lock().unwrap() = terminal;
    for change in [1, 2, 3, 4, 5, 6] {
        state.change.store(change, Ordering::SeqCst);
        let id = Uuid::new_v4();
        assert!(
            service
                .reconcile_bound_captured_payment(&scope, "payment", actor(), id)
                .await
                .is_err()
        );
        assert!(store.scoped_capture_receipt(id).await.unwrap().is_none());
        assert_eq!(audit_count(&owner, &role).await, 0);
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='order'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"pi_owned\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
        sqlx::query("UPDATE fl_ecommerce.payment_provider_bindings SET merchant_id='acct_owned' WHERE tenant_id=$1").bind(&role).execute(&owner).await.unwrap();
    }
    state.change.store(0, Ordering::SeqCst);
    // A current local succeeded record cannot be rewritten, even with no captured audit.
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"succeeded\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    let before = state.calls.load(Ordering::SeqCst);
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), before);
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"pending\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    sqlx::query("INSERT INTO fl_ecommerce.payment_reconciliation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,capture_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,'payment','pending','stripe','pi_owned','ch_prior',105,'USD','operator','fixture')").bind(Uuid::new_v4().to_string()).bind(&role).execute(&owner).await.unwrap();
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let foreign = PostgresEcommerceStore::new(
        pool.clone(),
        TenantContext::new(format!("foreign-{role}")).unwrap(),
        keys.clone(),
        actor(),
    )
    .await
    .unwrap();
    assert!(foreign.captured_payment_snapshot(&binding).await.is_err());
    for kind in ["payment", "order"] {
        // Existing records_check already prevents key/body identity substitution.
        assert!(sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{id}','\"substituted\"') WHERE tenant_id=$1 AND kind=$2").bind(&role).bind(kind).execute(&owner).await.is_err());
        assert!(store.captured_payment_snapshot(&binding).await.is_ok());
    }
    // Payment snapshot status changes, even pending->failed, must not be adopted.
    let snapshot = store.captured_payment_snapshot(&binding).await.unwrap();
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"failed\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    assert!(
        store
            .record_scoped_capture(
                snapshot,
                evidence(),
                "txn_owned",
                actor(),
                Uuid::new_v4(),
                || Ok(())
            )
            .await
            .is_err()
    );
    reset_capture(&owner, &role).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.payment_reconciliation_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let audit_failure = Uuid::new_v4();
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), audit_failure)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_capture_receipt(audit_failure)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.payment_reconciliation_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    // Receipt INSERT denial occurs after terminal audit INSERT; both must roll back immediately.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_ecommerce.scoped_payment_capture_receipts FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    let denied_id = Uuid::new_v4();
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), denied_id)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_capture_receipt(denied_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_ecommerce.scoped_payment_capture_receipts TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    // Expiry after the writes but before COMMIT rolls back audit, receipt and status.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = store.captured_payment_snapshot(&binding).await.unwrap();
    let checks = AtomicUsize::new(0);
    let stale_id = Uuid::new_v4();
    assert!(
        store
            .record_scoped_capture(snapshot, evidence(), "txn_owned", actor(), stale_id, || {
                if checks.fetch_add(1, Ordering::SeqCst) >= 2 {
                    Err(BlueprintError::new("expired", "Expired"))
                } else {
                    Ok(())
                }
            })
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 0);
    assert!(
        store
            .scoped_capture_receipt(stale_id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        service.base().get_payment("payment").await.unwrap().status,
        PaymentStatus::Pending
    );
    for denied_actor in [
        AuditContext::actor("", "fixture"),
        AuditContext::actor("  ", "fixture"),
    ] {
        let calls = state.calls.load(Ordering::SeqCst);
        assert!(
            service
                .reconcile_bound_captured_payment(&scope, "payment", denied_actor, Uuid::new_v4())
                .await
                .is_err()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), calls);
    }
    sqlx::query("INSERT INTO fl_ecommerce.payment_cancellation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,canceled_at,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,'payment','pending','stripe','pi_owned',1700000000,105,'USD','operator','fixture')").bind(Uuid::new_v4().to_string()).bind(&role).execute(&owner).await.unwrap();
    // A restored noncancelled order cannot erase retained cancellation evidence.
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    let receipt_id = Uuid::new_v4();
    let outcome = service
        .reconcile_bound_captured_payment(&scope, "payment", actor(), receipt_id)
        .await
        .unwrap();
    let receipt = match outcome {
        ScopedCaptureOutcome::Recorded { receipt, payment } => {
            assert_eq!(payment.status, PaymentStatus::Succeeded);
            assert_eq!(payment.provider_ref.as_deref(), Some("pi_owned"));
            receipt
        }
        ScopedCaptureOutcome::RecordedEvidenceNoLongerCurrent { .. } => {
            panic!("fresh receipt expected")
        }
    };
    assert_eq!(
        store.scoped_capture_receipt(receipt_id).await.unwrap(),
        Some(receipt)
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    // Reusing a receipt UUID rolls back the attempted second terminal audit.
    assert!(
        service
            .reconcile_bound_captured_payment(&scope, "payment", actor(), receipt_id)
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    // A real second payment cannot reuse the first receipt UUID; its audit/status roll back.
    let second = service
        .base()
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("second".into()),
                order_id: "order".into(),
                amount_cents: 105,
                currency: "usd".into(),
            },
            Some("pi_second".into()),
        )
        .await
        .unwrap();
    let second_binding = store
        .provision_payment_provider_binding(&scope, &second)
        .await
        .unwrap();
    let second_snapshot = store
        .captured_payment_snapshot(&second_binding)
        .await
        .unwrap();
    let mut second_evidence = evidence();
    second_evidence.provider_ref = "pi_second".into();
    second_evidence.capture_id = "ch_second".into();
    assert!(
        store
            .record_scoped_capture(
                second_snapshot,
                second_evidence,
                "txn_second",
                actor(),
                receipt_id,
                || Ok(())
            )
            .await
            .is_err()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    assert_eq!(
        service.base().get_payment("second").await.unwrap().status,
        PaymentStatus::Pending
    );
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
            .scoped_capture_receipt(receipt_id)
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
        sqlx::query("DELETE FROM fl_ecommerce.scoped_payment_capture_receipts")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    tx.rollback().await.unwrap();
    // Reset fixture-only owner data for an independent failed->succeeded publication.
    sqlx::query("DELETE FROM fl_ecommerce.scoped_payment_capture_receipts WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("DELETE FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{status}','\"failed\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    // Simulate deadline expiry only after COMMIT: return a written receipt, not rollback.
    let binding = store
        .payment_provider_binding(&scope, "payment")
        .await
        .unwrap();
    let snapshot = store.captured_payment_snapshot(&binding).await.unwrap();
    let checks = AtomicUsize::new(0);
    let expired_id = Uuid::new_v4();
    let result = store
        .record_scoped_capture(
            snapshot,
            evidence(),
            "txn_owned",
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
        ScopedCaptureOutcome::RecordedEvidenceNoLongerCurrent { .. }
    ));
    assert!(
        store
            .scoped_capture_receipt(expired_id)
            .await
            .unwrap()
            .is_some()
    );
    // Independent fixture reset, then concurrent snapshots allow exactly one publication.
    reset_capture(&owner, &role).await;
    let (first, second) = tokio::join!(
        service.reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4()),
        service.reconcile_bound_captured_payment(&scope, "payment", actor(), Uuid::new_v4())
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(audit_count(&owner, &role).await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT data->>'status' FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order'"
        )
        .bind(&role)
        .fetch_one(&owner)
        .await
        .unwrap(),
        "pending"
    );
    reset_capture(&owner, &role).await;
    // Actual PostgreSQL COMMIT succeeds; proxy discards its acknowledgement.
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
    let snapshot = uncertain.captured_payment_snapshot(&binding).await.unwrap();
    let unknown_id = Uuid::new_v4();
    armed.store(true, Ordering::SeqCst);
    let outcome = uncertain
        .record_scoped_capture(
            snapshot,
            evidence(),
            "txn_owned",
            actor(),
            unknown_id,
            || Ok(()),
        )
        .await;
    assert_eq!(
        outcome.err().unwrap().code,
        "ecommerce_capture_commit_unknown"
    );
    assert!(
        store
            .scoped_capture_receipt(unknown_id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(audit_count(&owner, &role).await, 1);
    proxied.close().await;
    proxy.abort();
    server.abort();
    pool.close().await;
    for table in [
        "scoped_payment_capture_receipts",
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

async fn reset_capture(owner: &PgPool, tenant: &str) {
    for table in [
        "scoped_payment_capture_receipts",
        "payment_reconciliation_audit",
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

struct Profile {
    service: StripePaymentService<PostgresEcommerceStore>,
    connected: bool,
}
impl std::ops::Deref for Profile {
    type Target = StripePaymentService<PostgresEcommerceStore>;
    fn deref(&self) -> &Self::Target {
        &self.service
    }
}
impl Profile {
    async fn reconcile_bound_captured_payment(
        &self,
        scope: &PaymentProviderScope,
        id: &str,
        actor: AuditContext,
        receipt: Uuid,
    ) -> forklaunch_core::domain::BlueprintResult<ScopedCaptureOutcome> {
        if self.connected {
            self.service
                .reconcile_bound_connected_captured_payment(scope, id, actor, receipt)
                .await
        } else {
            self.service
                .reconcile_bound_captured_payment(scope, id, actor, receipt)
                .await
        }
    }
}
