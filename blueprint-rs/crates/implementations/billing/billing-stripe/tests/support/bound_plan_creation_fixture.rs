use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{PostgresBillingStore, StripeBindingScope},
};

use forklaunch_billing_stripe::{
    bound_plan_creation::{BoundPlanCreateRequest, BoundPlanCreationProfile},
    client::{STRIPE_API_VERSION, StripeBillingClient},
    services::StripeBillingService,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::persistence::{TenantContext, encryption::FieldEncryptor};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;
#[derive(Clone)]
pub struct Provider {
    pub posts: Arc<AtomicUsize>,
    pub requests: Arc<AtomicUsize>,
    pub response: Arc<Mutex<Option<Value>>>,
    pub keys: Arc<Mutex<Vec<String>>>,
    pub product: Arc<Mutex<Value>>,
    pub readback: Arc<Mutex<Option<Value>>>,
    pub pause: Arc<AtomicBool>,
    pub pause_read: Arc<AtomicBool>,
    pub entered: Arc<Notify>,
    pub resume: Arc<Notify>,
    pub delay: Arc<AtomicU64>,
}
fn auth(headers: &HeaderMap) {
    assert_eq!(headers["authorization"], "Bearer opaque-bound-creation-key");
    assert_eq!(headers["stripe-version"], STRIPE_API_VERSION);
    assert!(!headers.contains_key("stripe-account"));
}
async fn create(State(state): State<Provider>, headers: HeaderMap, body: String) -> Json<Value> {
    auth(&headers);
    let key = headers["idempotency-key"].to_str().unwrap();
    assert!(key.starts_with("fl-plan-"));
    assert!(key.len() <= 255);
    state.keys.lock().unwrap().push(key.into());
    let fields: std::collections::BTreeMap<String, String> =
        reqwest::Url::parse(&format!("http://fixture/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
    let expected: std::collections::BTreeMap<String, String> = [
        ("product", "custom-product"),
        ("amount", "1200"),
        ("currency", "usd"),
        ("interval", "month"),
        ("interval_count", "1"),
        ("billing_scheme", "per_unit"),
        ("usage_type", "licensed"),
        ("active", "true"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    assert_eq!(fields, expected);
    let number = state.posts.fetch_add(1, Ordering::SeqCst) + 1;
    if state.pause.load(Ordering::SeqCst) {
        state.entered.notify_one();
        state.resume.notified().await;
    }
    tokio::time::sleep(Duration::from_millis(state.delay.load(Ordering::SeqCst))).await;
    Json(
        state
            .response
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| plan(&format!("custom-plan-{number}"))),
    )
}
pub struct Fixture {
    pub owner: PgPool,
    pub pool: PgPool,
    pub role: String,
    pub tenant: TenantContext,
    pub store: Arc<PostgresBillingStore>,
    pub service: Arc<StripeBillingService<PostgresBillingStore>>,
    pub profile: BoundPlanCreationProfile,
    pub scope: StripeBindingScope,
    pub provider: Provider,
    pub client: StripeBillingClient,
    pub server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    // Keep the sequential fixture lifecycle and its assertions together.
    #[allow(clippy::too_many_lines)]
    pub async fn new() -> Self {
        let owner = PgPoolOptions::new()
            .max_connections(4)
            .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
            .await
            .unwrap();
        PostgresBillingStore::migrate(&owner).await.unwrap();
        let role = format!("fl_bound_plan_{}", Uuid::new_v4().simple());
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-fixture'; GRANT USAGE ON SCHEMA fl_billing TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings,fl_billing.provider_binding_audit,fl_billing.sensitive_read_audit,fl_billing.plan_creation_intents,fl_billing.plan_creation_audit TO {role}"))).execute(&owner).await.unwrap();
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(
                (*owner.connect_options())
                    .clone()
                    .username(&role)
                    .password("owned-fixture"),
            )
            .await
            .unwrap();
        let tenant = TenantContext::new(format!("{role}-a")).unwrap();
        let store = Arc::new(
            PostgresBillingStore::new(
                pool.clone(),
                tenant.clone(),
                Arc::new(FieldEncryptor::new("owned-bound-create-encryption-key").unwrap()),
                AuditContext::actor("service:bound-create", "owned-creation-test"),
            )
            .await
            .unwrap(),
        );
        let provider = Provider {
            posts: Arc::new(AtomicUsize::new(0)),
            requests: Arc::new(AtomicUsize::new(0)),
            response: Arc::new(Mutex::new(None)),
            keys: Arc::new(Mutex::new(vec![])),
            product: Arc::new(Mutex::new(product())),
            readback: Arc::new(Mutex::new(None)),
            pause: Arc::new(AtomicBool::new(false)),
            pause_read: Arc::new(AtomicBool::new(false)),
            entered: Arc::new(Notify::new()),
            resume: Arc::new(Notify::new()),
            delay: Arc::new(AtomicU64::new(0)),
        };
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
            .route("/v1/plans", post(create))
            .route(
                "/v1/products/{id}",
                get(|State(state): State<Provider>, h: HeaderMap| async move {
                    auth(&h);
                    Json(state.product.lock().unwrap().clone())
                }),
            )
            .route(
                "/v1/plans/{id}",
                get(
                    |State(state): State<Provider>,
                     axum::extract::Path(id): axum::extract::Path<String>,
                     h: HeaderMap| async move {
                        auth(&h);
                        if state.pause_read.load(Ordering::SeqCst) {
                            state.entered.notify_one();
                            state.resume.notified().await;
                        }
                        Json(
                            state
                                .readback
                                .lock()
                                .unwrap()
                                .clone()
                                .unwrap_or_else(|| plan(&id)),
                        )
                    },
                ),
            )
            .with_state(provider.clone())
            .layer(axum::middleware::from_fn({
                let count = provider.requests.clone();
                move |r: axum::extract::Request, n: axum::middleware::Next| {
                    let count = count.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        n.run(r).await
                    }
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = StripeBillingClient::with_transport(
            "opaque-bound-creation-key".into(),
            ProviderHttpClient::new(&base, Duration::from_secs(2), 1024 * 1024).unwrap(),
        )
        .unwrap();
        let service = Arc::new(StripeBillingService::new(
            BaseBillingService::new(store.clone()),
            client.clone(),
        ));
        let scope = StripeBindingScope::administrative(
            "owned-deployment".into(),
            "acct_owned".into(),
            false,
        )
        .unwrap();
        let profile =
            BoundPlanCreationProfile::trusted(scope.clone(), vec!["custom-product".into()])
                .unwrap();
        Self {
            owner,
            pool,
            role,
            tenant,
            store,
            service,
            profile,
            scope,
            provider,
            client,
            server,
        }
    }
    pub fn posts(&self) -> usize {
        self.provider.posts.load(Ordering::SeqCst)
    }
    pub async fn counts(&self) -> (i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT count(*) FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable'),(SELECT count(*) FROM fl_billing.provider_bindings WHERE tenant_id=$1),(SELECT count(*) FROM fl_billing.plan_creation_intents WHERE tenant_id=$1 AND state='completed')").bind(self.tenant.id()).fetch_one(&self.owner).await.unwrap()
    }
    pub async fn cleanup(self) {
        self.server.abort();
        for table in [
            "plan_creation_audit",
            "plan_creation_intents",
            "provider_binding_audit",
            "sensitive_read_audit",
            "provider_bindings",
            "records",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM fl_billing.{table} WHERE tenant_id LIKE $1"
            )))
            .bind(format!("{}%", self.role))
            .execute(&self.owner)
            .await
            .unwrap();
        }
        self.pool.close().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP OWNED BY {}; DROP ROLE {}",
            self.role, self.role
        )))
        .execute(&self.owner)
        .await
        .unwrap();
        self.owner.close().await;
    }
}
pub fn product() -> Value {
    json!({"id":"custom-product","object":"product","livemode":false})
}
pub fn plan(id: &str) -> Value {
    json!({"id":id,"object":"plan","livemode":false,"product":product(),"amount":1200,"currency":"usd","interval":"month","interval_count":1,"billing_scheme":"per_unit","usage_type":"licensed","active":true})
}
pub fn request() -> BoundPlanCreateRequest {
    serde_json::from_value(json!({"name":"Owned plan","unitAmount":1200,"currency":"USD","cadence":"month","productId":"custom-product","active":true})).unwrap()
}
