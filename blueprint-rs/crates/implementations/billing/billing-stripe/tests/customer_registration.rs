#![cfg(feature = "postgres-identity")]
#[path = "support/commit_ack_proxy.rs"]
mod commit_ack_proxy;
#[allow(dead_code)]
#[path = "support/bound_creation_fixture.rs"]
mod fixture;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, Uri},
    routing::get,
};
use fixture::Fixture;
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{PostgresBillingStore, TrustedCustomerParty},
};
use forklaunch_billing_interface::customers::{
    CreateCustomerDto, CustomerService, UpdateCustomerDto,
};
use forklaunch_billing_stripe::{
    client::{STRIPE_API_VERSION, StripeBillingClient},
    customer_registration::CustomerRegistrationOutcome as Outcome,
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;
#[derive(Clone)]
struct Provider {
    response: Arc<Mutex<Value>>,
    calls: Arc<Mutex<Vec<String>>>,
    pause: Arc<AtomicBool>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
async fn provider(
    State(state): State<Provider>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Json<Value> {
    assert_eq!(method, Method::GET);
    assert_eq!(headers["authorization"], "Bearer opaque-customer-key");
    assert_eq!(headers["stripe-version"], STRIPE_API_VERSION);
    assert!(!headers.contains_key("stripe-account"));
    assert!(!headers.contains_key("idempotency-key"));
    state.calls.lock().unwrap().push(uri.path().into());
    Json(match uri.path() {
        "/v1/account" => json!({"id":"acct_owned","object":"account"}),
        "/v1/balance" => json!({"object":"balance","livemode":false}),
        "/v1/customers/cus_owned" => {
            if state.pause.load(Ordering::SeqCst) {
                state.entered.notify_one();
                state.resume.notified().await;
            }
            state.response.lock().unwrap().clone()
        }
        _ => panic!("unexpected provider path"),
    })
}
fn good() -> Value {
    json!({"id":"cus_owned","object":"customer","livemode":false,"email":"different-provider-email@example.test"})
}
async fn local_customer(f: &Fixture) -> Uuid {
    let row = BaseBillingService::new(f.store.clone())
        .create_customer(CreateCustomerDto {
            id: None,
            email: "local-only@example.test".into(),
            name: Some("private-local-name".into()),
        })
        .await
        .unwrap();
    Uuid::parse_str(&row.id).unwrap()
}
fn party(id: Uuid) -> TrustedCustomerParty {
    TrustedCustomerParty::administrative(
        id,
        "user".into(),
        "external-party-asserted-by-server".into(),
    )
    .unwrap()
}
async fn counts(f: &Fixture) -> (i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM fl_billing.stripe_customer_bindings WHERE tenant_id=$1),(SELECT count(*) FROM fl_billing.stripe_customer_binding_audit WHERE tenant_id=$1)").bind(f.tenant.id()).fetch_one(&f.owner).await.unwrap()
}
#[tokio::test]
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL"]
async fn direct_customer_registration_matrix() {
    let f = Fixture::new().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.stripe_customer_bindings,fl_billing.stripe_customer_binding_audit TO {}",f.role))).execute(&f.owner).await.unwrap();
    let provider = Provider {
        response: Arc::new(Mutex::new(good())),
        calls: Arc::new(Mutex::new(vec![])),
        pause: Arc::new(AtomicBool::new(false)),
        entered: Arc::new(Notify::new()),
        resume: Arc::new(Notify::new()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/v1/account", get(self::provider))
        .route("/v1/balance", get(self::provider))
        .route("/v1/customers/{id}", get(self::provider))
        .with_state(provider.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeBillingClient::with_transport(
        "opaque-customer-key".into(),
        ProviderHttpClient::new(&url, Duration::from_secs(2), 16384).unwrap(),
    )
    .unwrap();
    let service = Arc::new(StripeBillingService::new(
        BaseBillingService::new(f.store.clone()),
        client.clone(),
    ));
    successful_association(&f, &service, &provider).await;
    denied_actor_and_audit(&f, &client, &service, &provider).await;
    expired_audit_write(&f, &service).await;
    stale_local_customer(&f, &service, &provider).await;
    expired_local_lock(&f, &service, &provider).await;
    lost_commit(&f, &client, &service, &provider).await;
    for table in ["stripe_customer_binding_audit", "stripe_customer_bindings"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_billing.{table} WHERE tenant_id=$1"
        )))
        .bind(f.tenant.id())
        .execute(&f.owner)
        .await
        .unwrap();
    }
    server.abort();
    f.cleanup().await;
}
// Keep one association lifecycle and its CAS/tenant assertions together.
#[allow(clippy::too_many_lines)]
async fn successful_association(
    f: &Fixture,
    service: &StripeBillingService<PostgresBillingStore>,
    provider: &Provider,
) {
    let customer = local_customer(f).await;
    let before_reads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1",
    )
    .bind(f.tenant.id())
    .fetch_one(&f.owner)
    .await
    .unwrap();
    let Outcome::Registered(binding) = service
        .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
        .await
        .unwrap()
    else {
        panic!("expected registered")
    };
    assert_eq!(binding.customer_id(), customer);
    assert_eq!(binding.provider_customer_id(), "cus_owned");
    assert_eq!(binding.party_id(), "external-party-asserted-by-server");
    let after_reads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1",
    )
    .bind(f.tenant.id())
    .fetch_one(&f.owner)
    .await
    .unwrap();
    assert_eq!(before_reads, after_reads);
    let Outcome::Registered(again) = service
        .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
        .await
        .unwrap()
    else {
        panic!("expected registered")
    };
    assert_eq!(again.generation(), binding.generation());
    assert!(
        BaseBillingService::new(f.store.clone())
            .delete_customer(&customer.to_string())
            .await
            .is_err()
    );
    let wrong =
        TrustedCustomerParty::administrative(customer, "organization".into(), "other-party".into())
            .unwrap();
    let before = counts(f).await;
    assert!(
        service
            .register_verified_stripe_customer(&f.scope, &wrong, "cus_owned")
            .await
            .is_err()
    );
    assert_eq!(counts(f).await, before);
    for (field, bad) in [
        ("id", json!("cus_wrong")),
        ("id", json!(null)),
        ("object", json!("account")),
        ("livemode", json!(true)),
        ("deleted", json!(true)),
        ("deleted", json!(null)),
        ("deleted", json!("false")),
    ] {
        let mut value = good();
        value[field] = bad;
        *provider.response.lock().unwrap() = value;
        assert!(
            service
                .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
                .await
                .is_err()
        );
        assert_eq!(counts(f).await, before);
    }
    *provider.response.lock().unwrap() = good();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','foreign',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_billing.stripe_customer_bindings")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);
    tx.rollback().await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true)")
        .bind(f.tenant.id())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE fl_billing.stripe_customer_bindings SET party_id='attacker'")
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    f.store
        .revoke_stripe_customer_binding(&binding)
        .await
        .unwrap();
    let Outcome::Registered(replacement) = service
        .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
        .await
        .unwrap()
    else {
        panic!("expected registered")
    };
    assert_ne!(replacement.generation(), binding.generation());
    assert!(
        f.store
            .revoke_stripe_customer_binding(&binding)
            .await
            .is_err()
    );
    f.store
        .revoke_stripe_customer_binding(&replacement)
        .await
        .unwrap();
}
async fn stale_local_customer(
    f: &Fixture,
    service: &Arc<StripeBillingService<PostgresBillingStore>>,
    provider: &Provider,
) {
    let customer = local_customer(f).await;
    let before = counts(f).await;
    provider.pause.store(true, Ordering::SeqCst);
    let service = service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        service
            .register_verified_stripe_customer(&scope, &party(customer), "cus_owned")
            .await
    });
    provider.entered.notified().await;
    BaseBillingService::new(f.store.clone())
        .update_customer(UpdateCustomerDto {
            id: customer.to_string(),
            email: None,
            name: Some(Some("changed-local-name".into())),
        })
        .await
        .unwrap();
    provider.resume.notify_one();
    provider.pause.store(false, Ordering::SeqCst);
    assert!(task.await.unwrap().is_err());
    assert_eq!(counts(f).await, before);
}
async fn expired_local_lock(
    f: &Fixture,
    service: &Arc<StripeBillingService<PostgresBillingStore>>,
    provider: &Provider,
) {
    let customer = local_customer(f).await;
    let before = counts(f).await;
    provider.pause.store(true, Ordering::SeqCst);
    let service = service.clone();
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        service
            .register_verified_stripe_customer(&scope, &party(customer), "cus_owned")
            .await
    });
    provider.entered.notified().await;
    let mut lock = f.owner.begin().await.unwrap();
    sqlx::query("SELECT id FROM fl_billing.records WHERE tenant_id=$1 AND kind='customer' AND id=$2 FOR UPDATE").bind(f.tenant.id()).bind(customer.to_string()).execute(&mut *lock).await.unwrap();
    provider.resume.notify_one();
    provider.pause.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(5100)).await;
    lock.rollback().await.unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(counts(f).await, before);
}
async fn lost_commit(
    f: &Fixture,
    client: &StripeBillingClient,
    service: &Arc<StripeBillingService<PostgresBillingStore>>,
    provider: &Provider,
) {
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
            AuditContext::actor("service:bound-create", "customer-lost-ack"),
        )
        .await
        .unwrap(),
    );
    let proxied = StripeBillingService::new(BaseBillingService::new(store), client.clone());
    let untouched = local_customer(f).await;
    let calls_before = provider.calls.lock().unwrap().len();
    armed.store(true, Ordering::SeqCst);
    assert!(
        proxied
            .register_verified_stripe_customer(&f.scope, &party(untouched), "cus_owned")
            .await
            .is_err()
    );
    assert!(!armed.load(Ordering::SeqCst));
    assert_eq!(provider.calls.lock().unwrap().len(), calls_before);
    assert!(
        f.store
            .stripe_customer_binding(&f.scope, untouched)
            .await
            .unwrap()
            .is_none()
    );
    let customer = local_customer(f).await;
    provider.pause.store(true, Ordering::SeqCst);
    let scope = f.scope.clone();
    let task = tokio::spawn(async move {
        proxied
            .register_verified_stripe_customer(&scope, &party(customer), "cus_owned")
            .await
    });
    provider.entered.notified().await;
    armed.store(true, Ordering::SeqCst);
    provider.resume.notify_one();
    provider.pause.store(false, Ordering::SeqCst);
    assert!(task.await.unwrap().is_err());
    assert!(!armed.load(Ordering::SeqCst));
    let binding = f
        .store
        .stripe_customer_binding(&f.scope, customer)
        .await
        .unwrap()
        .unwrap();
    let Outcome::Registered(again) = service
        .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
        .await
        .unwrap()
    else {
        panic!("inspect replay")
    };
    assert_eq!(binding.generation(), again.generation());
    f.store
        .revoke_stripe_customer_binding(&binding)
        .await
        .unwrap();
    pool.close().await;
    proxy.abort();
}

async fn denied_actor_and_audit(
    f: &Fixture,
    client: &StripeBillingClient,
    service: &StripeBillingService<PostgresBillingStore>,
    provider: &Provider,
) {
    let customer = local_customer(f).await;
    let before = counts(f).await;
    let calls = provider.calls.lock().unwrap().len();
    for actor in [
        AuditContext::system("no-actor"),
        AuditContext::actor(" ", "blank-actor"),
    ] {
        let store = Arc::new(f.store.with_audit_context(actor).unwrap());
        let denied = StripeBillingService::new(BaseBillingService::new(store), client.clone());
        assert!(
            denied
                .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
                .await
                .is_err()
        );
    }
    assert_eq!(provider.calls.lock().unwrap().len(), calls);
    assert_eq!(counts(f).await, before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.stripe_customer_binding_audit FROM {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        service
            .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
            .await
            .is_err()
    );
    assert_eq!(counts(f).await, before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.stripe_customer_binding_audit TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        f.store
            .stripe_customer_binding(&f.scope, customer)
            .await
            .unwrap()
            .is_none()
    );
}

async fn expired_audit_write(f: &Fixture, service: &StripeBillingService<PostgresBillingStore>) {
    let customer = local_customer(f).await;
    let before = counts(f).await;
    let name = format!("customer_wait_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION fl_billing.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_id='{}' THEN PERFORM pg_sleep(5.1); END IF; RETURN NEW; END $$; CREATE TRIGGER {name} BEFORE INSERT ON fl_billing.stripe_customer_binding_audit FOR EACH ROW EXECUTE FUNCTION fl_billing.{name}()",f.tenant.id()))).execute(&f.owner).await.unwrap();
    assert!(
        service
            .register_verified_stripe_customer(&f.scope, &party(customer), "cus_owned")
            .await
            .is_err()
    );
    assert_eq!(counts(f).await, before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER {name} ON fl_billing.stripe_customer_binding_audit; DROP FUNCTION fl_billing.{name}()" ))).execute(&f.owner).await.unwrap();
}
