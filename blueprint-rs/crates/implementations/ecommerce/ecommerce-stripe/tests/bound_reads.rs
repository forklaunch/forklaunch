#![cfg(feature = "postgres")]
use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    postgres::{PaymentProviderScope, PostgresEcommerceStore},
};
use forklaunch_ecommerce_interface::{CreatePaymentDto, PaymentService};
use forklaunch_ecommerce_stripe::{StripeClient, StripeConnect, StripePaymentService};
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
    connected: bool,
    change: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    context_calls: Arc<AtomicUsize>,
    store: Arc<PostgresEcommerceStore>,
    scope: PaymentProviderScope,
    pool: sqlx::PgPool,
    payment: forklaunch_ecommerce_interface::PaymentDto,
}
async fn intent(State(s): State<Fixture>, headers: HeaderMap) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-secret");
    assert_eq!(
        headers.get("stripe-account").map(|v| v.to_str().unwrap()),
        s.connected.then_some("acct_owned")
    );
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
    Json(s.response.lock().unwrap().clone())
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
#[allow(clippy::too_many_lines)]
async fn bound_direct_read_scopes_generation_and_never_mutates_payment() {
    run_profile(false).await;
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn bound_connected_read_scopes_generation_and_never_mutates_payment() {
    run_profile(true).await;
}
#[allow(clippy::too_many_lines)]
async fn run_profile(connected_profile: bool) {
    let owner = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresEcommerceStore::migrate(&owner).await.unwrap();
    let role = format!("ecom_inspect_{}", uuid::Uuid::new_v4().simple());
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
    let expected = json!({"id":"pi_owned","object":"payment_intent","livemode":false,"amount":105,"currency":"usd","metadata":{"orderId":"owned-order"},"status":"canceled","client_secret":"never-return","customer":"private-customer"});
    let state = Fixture {
        response: Arc::new(Mutex::new(expected.clone())),
        connected: connected_profile,
        change: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
        context_calls: Arc::new(AtomicUsize::new(0)),
        store: store.clone(),
        scope: scope.clone(),
        pool: pool.clone(),
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
                s.context_calls.fetch_add(1, Ordering::SeqCst);
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
                s.context_calls.fetch_add(1, Ordering::SeqCst);
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route("/v1/payment_intents/pi_owned", get(intent))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeClient::with_transport(
        "owned-secret".into(),
        ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(2), 2048)
            .unwrap(),
    )
    .unwrap();
    let service = Profile::new(base, client.clone(), connected_profile);
    // Profile selection is explicit; neither API silently falls back.
    if connected_profile {
        assert!(
            service
                .service
                .inspect_bound_payment_intent(&scope, "owned-payment")
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
                .inspect_bound_payment_intent(&wrong_scope, "owned-payment")
                .await
                .is_err()
        );
    } else {
        assert!(
            service
                .service
                .inspect_bound_connected_payment_intent(&scope, "owned-payment")
                .await
                .is_err()
        );
    }
    assert_eq!(state.context_calls.load(Ordering::SeqCst), 0);
    let observation = service
        .inspect_bound_payment_intent(&scope, "owned-payment")
        .await
        .unwrap();
    assert_eq!(observation.status, "canceled");
    let serialized = serde_json::to_string(&observation).unwrap();
    assert!(!serialized.contains("never-return"));
    assert!(!serialized.contains("private-customer"));
    for (key, value) in [
        ("id", json!("pi_other")),
        ("object", json!("wrong")),
        ("livemode", json!(true)),
        ("amount", json!(106)),
        ("currency", json!("USD")),
        ("currency", json!("eur")),
        ("metadata", json!({"orderId":"other"})),
        ("status", json!("unknown")),
    ] {
        let mut invalid = expected.clone();
        invalid[key] = value;
        *state.response.lock().unwrap() = invalid;
        assert!(
            service
                .inspect_bound_payment_intent(&scope, "owned-payment")
                .await
                .is_err()
        );
    }
    *state.response.lock().unwrap() = expected;
    state.change.store(1, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_payment_intent(&scope, "owned-payment")
            .await
            .is_err()
    );
    state.change.store(0, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_payment_intent(&scope, "owned-payment")
            .await
            .is_ok()
    );
    state.change.store(2, Ordering::SeqCst);
    assert!(
        service
            .inspect_bound_payment_intent(&scope, "owned-payment")
            .await
            .is_err()
    );
    state.change.store(0, Ordering::SeqCst);
    // Successful inspection needs only SELECT; no audit or payment mutation is attempted.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_ecommerce.records,fl_ecommerce.payment_provider_bindings,fl_ecommerce.payment_provider_binding_audit FROM {role}")))
        .execute(&owner).await.unwrap();
    assert!(
        service
            .inspect_bound_payment_intent(&scope, "owned-payment")
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
    let other_service = Profile::new(
        BaseEcommerceService::new(other),
        client.clone(),
        connected_profile,
    );
    let count = state.calls.load(Ordering::SeqCst);
    assert!(
        other_service
            .inspect_bound_payment_intent(&scope, "owned-payment")
            .await
            .is_err()
    );
    let connected = StripePaymentService::new(BaseEcommerceService::new(store.clone()), client)
        .with_connect(StripeConnect::new("acct_owned".into(), 0).unwrap());
    let context_count = state.context_calls.load(Ordering::SeqCst);
    assert!(
        connected
            .inspect_bound_payment_intent(&scope, "owned-payment")
            .await
            .is_err()
    );
    assert_eq!(state.calls.load(Ordering::SeqCst), count);
    assert_eq!(state.context_calls.load(Ordering::SeqCst), context_count);
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
    async fn inspect_bound_payment_intent(
        &self,
        scope: &PaymentProviderScope,
        id: &str,
    ) -> forklaunch_core::domain::BlueprintResult<
        forklaunch_ecommerce_stripe::bound_reads::BoundPaymentIntentObservation,
    > {
        if self.connected {
            self.service
                .inspect_bound_connected_payment_intent(scope, id)
                .await
        } else {
            self.service.inspect_bound_payment_intent(scope, id).await
        }
    }
    fn base(&self) -> &BaseEcommerceService<PostgresEcommerceStore> {
        self.service.base()
    }
}
