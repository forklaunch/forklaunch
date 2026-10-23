#![cfg(feature = "bound-checkout")]
#[path = "support/bound_subscription_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use forklaunch_billing_base::services::postgres::TrustedSubscriptionPublisher;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::ops::Deref;
#[path = "support/subscription_read_provider.rs"]
#[allow(dead_code)]
mod provider;
struct Harness {
    db: fixture::Fixture,
    provider: provider::Provider,
    client: StripeBillingClient,
    publisher_pool: PgPool,
    role: String,
    server: tokio::task::JoinHandle<()>,
}
impl Deref for Harness {
    type Target = fixture::Fixture;
    fn deref(&self) -> &Self::Target {
        &self.db
    }
}
impl Harness {
    async fn new() -> Self {
        let db = fixture::Fixture::new().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",db.publisher_role,db.publisher_role,db.publisher_role))).execute(&db.owner).await.unwrap();
        let publisher_pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(
                (*db.pool.connect_options())
                    .clone()
                    .username(&db.publisher_role),
            )
            .await
            .unwrap();
        let role = db.pool.connect_options().get_username().to_owned();
        let (provider, client, server) = provider::start().await;
        Self {
            db,
            provider,
            client,
            publisher_pool,
            role,
            server,
        }
    }
    async fn publisher(&self) -> TrustedSubscriptionPublisher {
        self.store
            .trusted_subscription_publisher(self.publisher_pool.clone())
            .await
            .unwrap()
    }
    fn posts(&self) -> usize {
        self.provider.posts.load(Ordering::SeqCst)
    }
    async fn cleanup(self) {
        self.server.abort();
        let _ = self.server.await;
        self.publisher_pool.close().await;
        self.db.cleanup().await;
    }
}

use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1,
    bound_subscription_recovery_contracts,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use jsonwebtoken::{Algorithm, Validation};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;
const KEY: &[u8] = b"billing-date-fixture";
fn config(f: &Harness) -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscriptionRecovery":{"authority":"internal_service_tenant_v1","plans":[{"localPlanId":f.plan,"creationOperationId":f.plan_operation}],"collectionPolicy":"automatic_default_incomplete_v1"}}).to_string()).unwrap()
}
fn sign(method: &str, path: &str, body: Option<&Value>, actor: &str) -> String {
    let encoded = body.map(Value::to_string);
    generate_hmac_auth_header(
        KEY,
        actor,
        method,
        path,
        encoded
            .as_deref()
            .map_or(SignedBody::Absent, SignedBody::Text),
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        &Uuid::new_v4().to_string(),
    )
    .unwrap()
}
fn authentication() -> HmacJwksAuth {
    let mut auth = HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    for route in bound_subscription_recovery_contracts() {
        auth = auth
            .with_hmac_keys_for_route(
                &route,
                [
                    ("default".into(), KEY.to_vec()),
                    ("other".into(), KEY.to_vec()),
                ],
            )
            .unwrap();
    }
    auth
}
#[tokio::test]
#[ignore = "owned restricted PostgreSQL roles, loopback HTTP/MCP and generated SDKs"]
async fn bound_subscription_recovery_http_mcp_sdk() {
    let f = Arc::new(Harness::new().await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
async fn call(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
    actor: &str,
) -> reqwest::Response {
    let mut request = client
        .request(method.parse().unwrap(), format!("{base}{path}"))
        .header("authorization", sign(method, path, body, actor));
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(body.to_string());
    }
    request.send().await.unwrap()
}

async fn seed(f: &Harness) -> (Uuid, Value) {
    use forklaunch_billing_base::services::BaseBillingService;
    use forklaunch_billing_stripe::{
        bound_subscription_creation::{
            BoundSubscriptionCreationOutcome, BoundSubscriptionCreationProfile,
            SubscriptionCollectionPolicy,
        },
        services::StripeBillingService,
    };
    use forklaunch_core::domain::AuditContext;
    let audit = AuditContext::actor("service:default", "recovery-fixture");
    let store = f.store.with_audit_context(audit.clone()).unwrap();
    let publisher = f.publisher().await.with_audit_context(audit).unwrap();
    let service =
        StripeBillingService::new(BaseBillingService::new(Arc::new(store)), f.client.clone());
    let profile = BoundSubscriptionCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation)],
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .unwrap();
    let operation = Uuid::new_v4();
    *f.provider.readback.lock().unwrap() = Some(json!({"ambiguous":"owned fixture"}));
    assert!(matches!(
        service
            .create_bound_subscription(&profile, &publisher, operation, provider::request(f))
            .await
            .unwrap(),
        BoundSubscriptionCreationOutcome::Unresolved { .. }
    ));
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('forklaunch.tenant_id','subscription-a',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let(key,created):(String,i64)=sqlx::query_as("SELECT idempotency_key,floor(extract(epoch FROM created_at))::bigint FROM fl_billing.subscription_creation_intents WHERE operation_id=$1::uuid").bind(operation.to_string()).fetch_one(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let mut subscription = provider::subscription(f.posts());
    subscription["created"] = json!(created);
    *f.provider.readback.lock().unwrap() = Some(subscription.clone());
    let event = json!({"id":"evt_owned","object":"event","type":"customer.subscription.created","api_version":forklaunch_billing_stripe::client::STRIPE_API_VERSION,"livemode":false,"created":created,"request":{"id":"req_owned","idempotency_key":key},"data":{"object":subscription},"account":null,"context":null});
    *f.provider.event.lock().unwrap() = event.clone();
    (operation, event)
}
async fn matrix(f: &Harness) {
    let (operation, event) = seed(f).await;
    let audit_store = f
        .store
        .with_audit_context(forklaunch_core::domain::AuditContext::actor(
            "service:default",
            "recovery-preflight",
        ))
        .unwrap();
    audit_store
        .snapshot_subscription_recovery(operation)
        .await
        .expect("seed recovery snapshot");
    let posts = f.posts();
    let before = f.provider.requests.load(Ordering::SeqCst);
    startup(f).await;
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT ON fl_billing.subscription_creation_intents,fl_billing.subscription_dependency_claims FROM {}",f.role))).execute(&f.owner).await.unwrap();
    let backend = BillingBackend::stripe((*f.store).clone(), f.client.clone())
        .with_bound_billing(
            BoundBillingV1::trusted_with_subscription(
                config(f),
                None,
                None,
                Some(f.publisher().await),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let router =
        BillingHttp::new(backend, authentication(), ServiceInfo::new("billing", "1")).router();
    let legacy = forklaunch_billing_interface::module_contracts::module_registry(true);
    assert_eq!(router.registry().routes().len(), legacy.routes().len() + 2);
    for route in legacy.routes() {
        assert!(router.registry().routes().contains(route));
    }
    assert!(
        !router
            .registry()
            .routes()
            .iter()
            .any(|r| r.operation_name == "createBoundSubscription")
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let path = format!("/bound-subscription-operations/{operation}/recover");
    let input = json!({"eventId":"evt_owned"});
    denied_requests(f, &client, &base, &path).await;
    audit_store
        .snapshot_subscription_recovery(operation)
        .await
        .expect("post startup recovery snapshot");
    denied_evidence(f, &client, &base, &path, &event).await;
    let records = f.count("records").await;
    let (a, b) = tokio::join!(
        call(&client, &base, "POST", &path, Some(&input), "default"),
        call(&client, &base, "POST", &path, Some(&input), "default")
    );
    assert!(a.status() == 200 || b.status() == 200);
    assert!([200, 202, 409].contains(&a.status().as_u16()));
    assert!([200, 202, 409].contains(&b.status().as_u16()));
    assert_eq!(f.count("records").await, records + 1);
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    let inspect = format!("/bound-subscription-operations/{operation}");
    let completed: Value = call(&client, &base, "GET", &inspect, None, "default")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed.as_object().unwrap().len(), 3);
    assert_eq!(
        call(&client, &base, "GET", &inspect, None, "other")
            .await
            .status(),
        409
    );
    assert_eq!(
        call(&client, &base, "POST", &path, Some(&input), "default")
            .await
            .json::<Value>()
            .await
            .unwrap(),
        completed
    );
    mcp(&client, &base, operation).await;
    sdks(f, &base, operation).await;
    assert_eq!(f.posts(), posts);
    server.abort();
    let _ = server.await;
}
async fn denied_requests(f: &Harness, client: &reqwest::Client, base: &str, path: &str) {
    let input = json!({"eventId":"evt_owned"});
    assert_eq!(
        client
            .post(format!("{base}{path}"))
            .json(&input)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        call(
            client,
            base,
            "POST",
            "/bound-subscriptions",
            Some(&json!({})),
            "default"
        )
        .await
        .status(),
        404
    );
    let requests = f.provider.requests.load(Ordering::SeqCst);
    for invalid in [
        json!({}),
        json!({"eventId":null}),
        json!({"eventId":"bad/path"}),
        json!({"eventId":"evt_owned","retry":true}),
    ] {
        assert_eq!(
            call(client, base, "POST", path, Some(&invalid), "default")
                .await
                .status(),
            400
        );
    }
    assert_eq!(
        call(client, base, "POST", path, Some(&input), "other")
            .await
            .status(),
        409
    );
    let missing = format!("/bound-subscription-operations/{}/recover", Uuid::new_v4());
    assert_eq!(
        call(client, base, "POST", &missing, Some(&input), "default")
            .await
            .status(),
        409
    );
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), requests);
}
async fn denied_evidence(
    f: &Harness,
    client: &reqwest::Client,
    base: &str,
    path: &str,
    event: &Value,
) {
    let records = f.count("records").await;
    for (pointer, value) in [
        ("/request", Value::Null),
        ("/request/idempotency_key", json!("wrong")),
        ("/api_version", json!("2019-02-19")),
        ("/account", json!("acct_other")),
        ("/livemode", json!(true)),
        ("/created", json!(event["created"].as_i64().unwrap() + 301)),
        ("/data/object/items/data/0/quantity", json!(3)),
    ] {
        let mut invalid = event.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        invalid["metadata"] = json!({"operation":"not-authority"});
        *f.provider.event.lock().unwrap() = invalid;
        assert_eq!(
            call(
                client,
                base,
                "POST",
                path,
                Some(&json!({"eventId":"evt_owned"})),
                "default"
            )
            .await
            .status(),
            400,
            "{pointer}"
        );
        assert_eq!(f.count("records").await, records);
        assert_eq!(f.count("subscription_dependency_claims").await, 4);
    }
    *f.provider.event.lock().unwrap() = event.clone();
}
async fn startup(f: &Harness) {
    assert!(BoundBillingV1::trusted(config(f), None, None).is_err());
    for backend in [
        BillingBackend::base((*f.store).clone()),
        BillingBackend::stripe((*f.store).clone(), f.client.clone()).with_database_backup(true),
        BillingBackend::stripe(
            (*f.store).clone(),
            StripeBillingClient::managed(
                "http://127.0.0.1:9",
                Uuid::new_v4().to_string(),
                "key".into(),
            )
            .unwrap(),
        ),
    ] {
        assert!(
            backend
                .with_bound_billing(
                    BoundBillingV1::trusted_with_subscription(
                        config(f),
                        None,
                        None,
                        Some(f.publisher().await)
                    )
                    .unwrap()
                )
                .await
                .is_err()
        );
    }
    for (table, privilege) in [
        ("subscription_creation_intents", "SELECT"),
        ("subscription_dependency_claims", "SELECT"),
        ("subscription_creation_audit", "INSERT"),
        ("records", "UPDATE"),
    ] {
        let publisher = f.publisher().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {}",
            f.role
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        assert!(
            BillingBackend::stripe((*f.store).clone(), f.client.clone())
                .with_bound_billing(
                    BoundBillingV1::trusted_with_subscription(
                        config(f),
                        None,
                        None,
                        Some(publisher)
                    )
                    .unwrap()
                )
                .await
                .is_err()
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT {privilege} ON fl_billing.{table} TO {}",
            f.role
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
}
async fn mcp(client: &reqwest::Client, base: &str, operation: Uuid) {
    let input = json!({"eventId":"evt_owned"});
    let path = format!("/bound-subscription-operations/{operation}/recover");
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"recoverBoundSubscriptionOperation","arguments":{"body":input,"params":{"operationId":operation},"headers":{"authorization":sign("POST",&path,Some(&input),"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
    let path = format!("/bound-subscription-operations/{operation}");
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspectBoundSubscriptionOperation","arguments":{"params":{"operationId":operation},"headers":{"authorization":sign("GET",&path,None,"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
}
async fn sdks(_f: &Harness, base: &str, operation: Uuid) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_subscription_recovery_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-subscription-recovery"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-subscription-recovery")
            .with_client_struct("BoundSubscriptionClient"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!(
        "fl-bound-subscription-recovery-sdk-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_subscription_recovery_route_sdks.mjs");
    let base = base.to_owned();
    let operation = operation.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts_path)
            .arg(rs_path)
            .arg(base)
            .arg(operation)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
