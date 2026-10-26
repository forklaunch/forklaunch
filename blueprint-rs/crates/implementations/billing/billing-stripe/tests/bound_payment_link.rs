#![cfg(feature = "postgres-identity")]
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_billing_base::services::{
    BaseBillingService, BillingKind, BillingStore, Scope,
    postgres::{PostgresBillingStore, StripeBindingScope},
};
use forklaunch_billing_stripe::{client::StripeBillingClient, services::StripeBillingService};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone)]
struct Provider {
    object: Arc<Mutex<Value>>,
    reads: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    pause: Arc<AtomicBool>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(headers: &HeaderMap) {
    assert_eq!(headers["authorization"], "Bearer opaque-owned-credential");
    assert!(!headers.contains_key("stripe-account"));
}
#[allow(clippy::too_many_lines)]
async fn provider(
    managed: bool,
) -> (
    StripeBillingClient,
    Provider,
    String,
    tokio::task::JoinHandle<()>,
) {
    let state = Provider {
        object: Arc::new(Mutex::new(
            json!({"id":"plink_remote","object":"payment_link","livemode":false,"url":"https://provider.test/current"}),
        )),
        reads: Arc::new(AtomicUsize::new(0)),
        requests: Arc::new(AtomicUsize::new(0)),
        pause: Arc::new(AtomicBool::new(false)),
        entered: Arc::new(Notify::new()),
        resume: Arc::new(Notify::new()),
    };
    let scope = json!({"version":1,"instanceId":"instance-owned","organizationId":"org-owned","accountId":"acct_owned","livemode":false,"bindingGeneration":"00000000-0000-4000-8000-000000000001","apiVersion":forklaunch_billing_stripe::client::STRIPE_API_VERSION,"issuedAt":10000,"expiresAt":15000});
    let app = Router::new()
        .route(
            "/v1/account",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"id":"acct_owned","object":"account"}))
            }),
        )
        .route(
            "/v1/balance",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route(
            "/v1/payment_links/plink_remote",
            get(|State(s): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                s.reads.fetch_add(1, Ordering::SeqCst);
                if s.pause.load(Ordering::SeqCst) {
                    s.entered.notify_one();
                    s.resume.notified().await;
                }
                Json(s.object.lock().unwrap().clone())
            }),
        )
        .route(
            "/instance-gateway/payments/context",
            post({
                let scope = scope.clone();
                move |h: HeaderMap| {
                    let scope = scope.clone();
                    async move {
                        assert!(
                            h["authorization"]
                                .to_str()
                                .unwrap()
                                .starts_with("HMAC keyId=instance-owned ")
                        );
                        Json(json!({"scope":scope,"contextToken":"owned-fixture-token"}))
                    }
                }
            }),
        )
        .route(
            "/instance-gateway/payments/context/payment-link-read",
            post(
                move |State(s): State<Provider>, h: HeaderMap, Json(body): Json<Value>| {
                    let scope = scope.clone();
                    async move {
                        assert!(
                            h["authorization"]
                                .to_str()
                                .unwrap()
                                .starts_with("HMAC keyId=instance-owned ")
                        );
                        assert_eq!(body["contextToken"], "owned-fixture-token");
                        assert_eq!(body["id"], "plink_remote");
                        s.reads.fetch_add(1, Ordering::SeqCst);
                        if s.pause.load(Ordering::SeqCst) {
                            s.entered.notify_one();
                            s.resume.notified().await;
                        }
                        Json(json!({"scope":scope,"object":s.object.lock().unwrap().clone()}))
                    }
                },
            ),
        )
        .layer(axum::middleware::from_fn({
            let requests = state.requests.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let requests = requests.clone();
                async move {
                    requests.fetch_add(1, Ordering::SeqCst);
                    assert!(
                        request.method() == axum::http::Method::GET
                            || (request.method() == axum::http::Method::POST
                                && matches!(
                                    request.uri().path(),
                                    "/instance-gateway/payments/context"
                                        | "/instance-gateway/payments/context/payment-link-read"
                                ))
                    );
                    next.run(request).await
                }
            }
        }))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = StripeBillingClient::with_transport(
        "opaque-owned-credential".into(),
        ProviderHttpClient::new(&base, Duration::from_secs(2), 1024 * 1024).unwrap(),
    )
    .unwrap();
    let client = if managed {
        StripeBillingClient::managed(&base, "instance-owned".into(), "owned-instance-key".into())
            .unwrap()
    } else {
        client
    };
    (client, state, base, server)
}

#[tokio::test]
#[ignore = "requires an isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
#[allow(clippy::too_many_lines)]
async fn direct_bound_payment_link_requires_live_context_and_durable_generation() {
    exercise(false).await;
}
#[tokio::test]
#[ignore = "requires an isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
async fn managed_bound_payment_link_requires_live_context_and_durable_generation() {
    exercise(true).await;
}
#[allow(clippy::too_many_lines)]
async fn exercise(managed: bool) {
    // GRANT/REVOKE update a shared PostgreSQL catalog row even for isolated roles.
    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = TEST_LOCK.lock().await;
    let owner = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    PostgresBillingStore::migrate(&owner).await.unwrap();
    let role = format!("fl_bound_link_{}", Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; GRANT USAGE ON SCHEMA fl_billing TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.records TO {role}; GRANT SELECT,INSERT,DELETE ON fl_billing.provider_bindings TO {role}; GRANT INSERT ON fl_billing.provider_binding_audit,fl_billing.sensitive_read_audit TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("owned-fixture"),
        )
        .await
        .unwrap();
    let tenant = TenantContext::new(format!("{role}-a")).unwrap();
    let keys = Arc::new(FieldEncryptor::new("owned-bound-read-fixture-key").unwrap());
    let audit = AuditContext::actor("service:bound-read-fixture", "bound-read-fixture");
    let store = Arc::new(
        PostgresBillingStore::new(pool.clone(), tenant.clone(), keys.clone(), audit.clone())
            .await
            .unwrap(),
    );
    let other = Arc::new(
        PostgresBillingStore::new(
            pool.clone(),
            TenantContext::new(format!("{role}-b")).unwrap(),
            keys,
            audit,
        )
        .await
        .unwrap(),
    );
    let (client, fixture, _provider_base, server) = provider(managed).await;
    let service = Arc::new(StripeBillingService::new(
        BaseBillingService::new(store.clone()),
        client.clone(),
    ));
    let other_service = StripeBillingService::new(BaseBillingService::new(other), client);
    let scope =
        StripeBindingScope::administrative("owned-deployment".into(), "acct_owned".into(), false)
            .unwrap();
    let public_id = Uuid::new_v4();

    store.insert(Scope::Durable,BillingKind::PaymentLink,json!({"id":"legacy-storage-key","amount":1200,"paymentMethods":["card"],"currency":"USD","status":"PENDING","expiresAt":"2030-01-01T00:00:00Z","uri":"https://local.test/private-token","providerFields":{"id":"plink_remote","private":"must-stay-encrypted"}}),Duration::from_secs(600)).await.unwrap();
    let original: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id='legacy-storage-key'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    let mut binding = store
        .provision_stripe_binding(
            &scope,
            BillingKind::PaymentLink,
            public_id,
            "legacy-storage-key",
            "plink_remote",
        )
        .await
        .unwrap();
    let result = service
        .get_bound_payment_link(&scope, public_id)
        .await
        .unwrap();
    assert_eq!(result.id, public_id.to_string());
    assert_eq!(result.amount.to_bits(), 1200.0_f64.to_bits());
    assert_eq!(result.status, "PENDING");
    assert_eq!(
        result.provider_fields.as_ref().unwrap()["id"],
        "plink_remote"
    );
    // No provider object read is permitted without tenant-scoped owned binding.
    let reads = fixture.reads.load(Ordering::SeqCst);
    assert!(
        service
            .get_bound_payment_link(&scope, Uuid::new_v4())
            .await
            .is_err()
    );
    assert!(
        other_service
            .get_bound_payment_link(&scope, public_id)
            .await
            .is_err()
    );
    for wrong in [
        StripeBindingScope::administrative("other-deployment".into(), "acct_owned".into(), false)
            .unwrap(),
        StripeBindingScope::administrative("owned-deployment".into(), "acct_other".into(), false)
            .unwrap(),
        StripeBindingScope::administrative("owned-deployment".into(), "acct_owned".into(), true)
            .unwrap(),
    ] {
        assert!(
            service
                .get_bound_payment_link(&wrong, public_id)
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.reads.load(Ordering::SeqCst), reads);
    let valid = fixture.object.lock().unwrap().clone();
    for (field, value) in [
        ("id", json!("plink_other")),
        ("object", json!("payment_intent")),
        ("livemode", json!(true)),
        ("livemode", Value::Null),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        *fixture.object.lock().unwrap() = invalid;
        assert!(
            service
                .get_bound_payment_link(&scope, public_id)
                .await
                .is_err()
        );
    }
    *fixture.object.lock().unwrap() = valid;
    // Revoke/reprovision while provider GET is in flight: an identical public
    // alias must not allow an old generation to return local customer data.
    fixture.pause.store(true, Ordering::SeqCst);
    let reader = service.clone();
    let reader_scope = scope.clone();
    let pending = tokio::spawn(async move {
        reader
            .get_bound_payment_link(&reader_scope, public_id)
            .await
    });
    fixture.entered.notified().await;
    store.revoke_stripe_binding(&binding).await.unwrap();
    binding = store
        .provision_stripe_binding(
            &scope,
            BillingKind::PaymentLink,
            public_id,
            "legacy-storage-key",
            "plink_remote",
        )
        .await
        .unwrap();
    fixture.resume.notify_one();
    assert!(pending.await.unwrap().is_err());
    // Expiry after a DB pool wait must fail at the final freshness check, even
    // though the provider read itself completed while context was fresh.
    let reader = service.clone();
    let reader_scope = scope.clone();
    let pending = tokio::spawn(async move {
        reader
            .get_bound_payment_link(&reader_scope, public_id)
            .await
    });
    fixture.entered.notified().await;
    let held_a = pool.acquire().await.unwrap();
    let held_b = pool.acquire().await.unwrap();
    fixture.resume.notify_one();
    tokio::time::sleep(Duration::from_millis(5100)).await;
    drop(held_a);
    drop(held_b);
    assert_eq!(
        pending.await.unwrap().unwrap_err().code,
        "billing_provider_context_mismatch"
    );
    fixture.pause.store(false, Ordering::SeqCst);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.sensitive_read_audit FROM {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        service
            .get_bound_payment_link(&scope, public_id)
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.sensitive_read_audit TO {role}"
    )))
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        service
            .get_bound_payment_link(&scope, public_id)
            .await
            .is_ok()
    );
    let after: sqlx::types::Json<Value> = sqlx::query_scalar(
        "SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND id='legacy-storage-key'",
    )
    .bind(tenant.id())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(after, original);
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE tenant_id=$1 AND actor_id='service:bound-read-fixture' AND operation='provider_binding_read'").bind(tenant.id()).fetch_one(&owner).await.unwrap();
    assert!(audits > 0);
    store.revoke_stripe_binding(&binding).await.unwrap();
    let reads = fixture.reads.load(Ordering::SeqCst);
    assert!(
        service
            .get_bound_payment_link(&scope, public_id)
            .await
            .is_err()
    );
    assert_eq!(fixture.reads.load(Ordering::SeqCst), reads);
    server.abort();
    drop(service);
    drop(other_service);
    drop(store);
    pool.close().await;
    for table in [
        "provider_bindings",
        "provider_binding_audit",
        "records",
        "sensitive_read_audit",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM fl_billing.{table} WHERE tenant_id=$1"
        )))
        .bind(tenant.id())
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
