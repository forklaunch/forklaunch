#![cfg(feature = "postgres")]
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    postgres::{PaymentProviderScope, PostgresEcommerceStore},
};
use forklaunch_ecommerce_interface::{CreatePaymentDto, PaymentService};
use forklaunch_ecommerce_paypal::{
    PaypalClient, PaypalOrderStatus, PaypalPaymentService, context::PaypalEnvironment,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
struct Fixture {
    response: Arc<Mutex<Value>>,
    change: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    context_calls: Arc<AtomicUsize>,
    store: Arc<PostgresEcommerceStore>,
    scope: PaymentProviderScope,
    pool: sqlx::PgPool,
    owner: sqlx::PgPool,
    tenant: String,
    payment: forklaunch_ecommerce_interface::PaymentDto,
}
async fn intent(State(s): State<Fixture>, headers: HeaderMap) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-token");
    assert!(!headers.contains_key("paypal-auth-assertion"));
    assert!(!headers.contains_key("paypal-partner-attribution-id"));
    s.calls.fetch_add(1, Ordering::SeqCst);
    if s.change.load(Ordering::SeqCst) == 1 {
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
    if s.change.load(Ordering::SeqCst) == 2 {
        let mut connections = Vec::new();
        for _ in 0..4 {
            connections.push(s.pool.acquire().await.unwrap());
        }
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5100)).await;
            drop(connections);
        });
    }
    if s.change.load(Ordering::SeqCst) == 3 {
        sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"ORDER_replaced\"') WHERE tenant_id=$1 AND kind='payment'").bind(&s.tenant).execute(&s.owner).await.unwrap();
    }
    Json(s.response.lock().unwrap().clone())
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn bound_paypal_order_scope_generation_and_no_mutation() {
    let owner = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("paypal_inspect_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'fixture-only'; GRANT USAGE ON SCHEMA fl_ecommerce TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit TO {role}"))).execute(&owner).await.unwrap();
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
    let keys = Arc::new(FieldEncryptor::new("owned-inspection-key").unwrap());
    let store = Arc::new(
        PostgresEcommerceStore::new(
            pool.clone(),
            TenantContext::new(role.clone()).unwrap(),
            keys.clone(),
            AuditContext::actor("operator", "inspect-fixture"),
        )
        .await
        .unwrap(),
    );
    sqlx::query("INSERT INTO fl_ecommerce.records(tenant_id,kind,id,data) VALUES($1,'order','owned-order','{\"id\":\"owned-order\",\"status\":\"cancelled\",\"privateAddress\":\"not-read\"}')").bind(&role).execute(&owner).await.unwrap();
    let base = BaseEcommerceService::new(store.clone());
    let payment = base
        .create_payment_with_reference(
            CreatePaymentDto {
                id: Some("owned-payment".into()),
                order_id: "owned-order".into(),
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
    let expected = json!({"id":"ORDER_owned","intent":"CAPTURE","status":"COMPLETED","purchase_units":[{"reference_id":"owned-order","amount":{"currency_code":"USD","value":"1.05"},"payee":{"merchant_id":"Q7C9N6S4YRQXG","email_address":"private@example.invalid"}}],"payer":{"secret":"never-return"}});
    let state = Fixture {
        response: Arc::new(Mutex::new(expected.clone())),
        change: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
        context_calls: Arc::new(AtomicUsize::new(0)),
        store: store.clone(),
        scope: scope.clone(),
        pool: pool.clone(),
        owner: owner.clone(),
        tenant: role.clone(),
        payment: payment.clone(),
    };
    let app=Router::new().route("/v1/oauth2/token",post(|State(s):State<Fixture>,headers:HeaderMap,body:String|async move{
        s.context_calls.fetch_add(1,Ordering::SeqCst);
        assert_eq!(headers["authorization"],"Basic b3duZWQtY2xpZW50Om93bmVkLXNlY3JldA==");
        assert_eq!(body,"grant_type=client_credentials");
        assert!(!headers.contains_key("paypal-auth-assertion"));
        Json(json!({"access_token":"owned-token","token_type":"Bearer","expires_in":300}))
    })).route("/v2/checkout/orders/ORDER_owned",get(intent)).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let transport =
        ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(2), 2048)
            .unwrap();
    let client = PaypalClient::with_trusted_environment_transport(
        "owned-client".into(),
        "owned-secret".into(),
        PaypalEnvironment::Sandbox,
        transport.clone(),
    )
    .unwrap();
    let service = PaypalPaymentService::new(base, client.clone());
    let unprofiled = PaypalPaymentService::new(
        BaseEcommerceService::new(store.clone()),
        PaypalClient::with_transport("owned-client".into(), "owned-secret".into(), transport)
            .unwrap(),
    );
    assert!(
        unprofiled
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_err()
    );
    let wrong_environment = PaymentProviderScope::administrative(
        "paypal".into(),
        "owned".into(),
        "Q7C9N6S4YRQXG".into(),
        true,
    )
    .unwrap();
    assert!(
        service
            .inspect_bound_order(&wrong_environment, "owned-payment")
            .await
            .is_err()
    );
    assert_eq!(state.context_calls.load(Ordering::SeqCst), 0);
    let observation = service
        .inspect_bound_order(&scope, "owned-payment")
        .await
        .unwrap();
    assert_eq!(observation.status, PaypalOrderStatus::Completed);
    let serialized = serde_json::to_string(&observation).unwrap();
    assert!(!serialized.contains("never-return"));
    assert!(!serialized.contains("private-customer"));
    for (pointer, value) in [
        ("/id", json!("other")),
        ("/status", json!("unknown")),
        ("/purchase_units/0/amount/value", json!("1.06")),
        ("/purchase_units/0/amount/currency_code", json!("EUR")),
        ("/purchase_units/0/reference_id", json!("other")),
        ("/purchase_units/0/payee/merchant_id", Value::Null),
        (
            "/purchase_units/0/payee/merchant_id",
            json!("J8NCBBULP5JH2"),
        ),
    ] {
        let mut invalid = expected.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        *state.response.lock().unwrap() = invalid;
        assert!(
            service
                .inspect_bound_order(&scope, "owned-payment")
                .await
                .is_err()
        );
    }
    *state.response.lock().unwrap() = expected;
    state.change.store(1, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_err()
    );
    state.change.store(0, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_ok()
    );
    state.change.store(2, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_err()
    );
    state.change.store(0, Ordering::SeqCst);
    state.change.store(3, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_err()
    );
    state.change.store(0, Ordering::SeqCst);
    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{providerRef}','\"ORDER_owned\"') WHERE tenant_id=$1 AND kind='payment'").bind(&role).execute(&owner).await.unwrap();
    // Successful inspection needs only SELECT; no audit or payment mutation is attempted.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit FROM {role}")))
        .execute(&owner).await.unwrap();
    assert!(
        service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_ok()
    );
    let other = Arc::new(
        PostgresEcommerceStore::new(
            pool.clone(),
            TenantContext::new(format!("other-{role}")).unwrap(),
            keys,
            AuditContext::actor("operator", "inspect-fixture"),
        )
        .await
        .unwrap(),
    );
    let other_service = PaypalPaymentService::new(BaseEcommerceService::new(other), client);
    let count = state.calls.load(Ordering::SeqCst);
    let contexts = state.context_calls.load(Ordering::SeqCst);
    assert!(
        other_service
            .inspect_bound_order(&scope, "owned-payment")
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), count);
    assert_eq!(state.context_calls.load(Ordering::SeqCst), contexts);
    assert_eq!(
        serde_json::to_value(service.base().get_payment("owned-payment").await.unwrap()).unwrap(),
        serde_json::to_value(payment).unwrap()
    );
    server.abort();
    pool.close().await;
    sqlx::query("DELETE FROM fl_ecommerce.records WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("DELETE FROM fl_ecommerce.payment_provider_binding_audit WHERE tenant_id=$1")
        .bind(&role)
        .execute(&owner)
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role}; DROP ROLE {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
}
