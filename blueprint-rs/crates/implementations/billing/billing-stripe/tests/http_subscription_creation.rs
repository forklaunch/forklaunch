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
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1, bound_subscription_contracts,
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
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscription":{"authority":"internal_service_tenant_v1","plans":[{"localPlanId":f.plan,"creationOperationId":f.plan_operation}],"collectionPolicy":"automatic_default_incomplete_v1"}}).to_string()).unwrap()
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
    for route in bound_subscription_contracts() {
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
async fn bound_subscription_creation_http_mcp_sdk() {
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
fn body(f: &Harness) -> Value {
    json!({"operationId":Uuid::new_v4().to_string(),"localCustomerId":f.customer,"localPlanId":f.plan,"quantity":2})
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
fn service() -> ServiceInfo {
    ServiceInfo::new("billing", "1")
}

async fn matrix(f: &Harness) {
    startup(f).await;
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
    let router = BillingHttp::new(backend, authentication(), service()).router();
    let legacy = forklaunch_billing_interface::module_contracts::module_registry(true);
    assert_eq!(router.registry().routes().len(), legacy.routes().len() + 2);
    for route in legacy.routes() {
        assert!(router.registry().routes().contains(route));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let input = body(f);
    let path = "/bound-subscriptions";
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
    let response = call(&client, &base, "POST", path, Some(&input), "default").await;
    assert_eq!(response.status(), 200);
    let completed: Value = response.json().await.unwrap();
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed.as_object().unwrap().len(), 3);
    assert_ne!(completed["publicId"], "sub_wire1");
    let again = call(&client, &base, "POST", path, Some(&input), "default")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(again, completed);
    let mut integral_float = input.clone();
    integral_float["quantity"] = json!(2.0);
    let response = call(
        &client,
        &base,
        "POST",
        path,
        Some(&integral_float),
        "default",
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap(), completed);
    assert_eq!(f.posts(), 1);
    let inspect = format!(
        "/bound-subscription-operations/{}",
        input["operationId"].as_str().unwrap()
    );
    assert_eq!(
        call(&client, &base, "GET", &inspect, None, "default")
            .await
            .json::<Value>()
            .await
            .unwrap(),
        completed
    );
    assert_eq!(
        call(&client, &base, "GET", &inspect, None, "other")
            .await
            .status(),
        409
    );
    let missing = format!("/bound-subscription-operations/{}", Uuid::new_v4());
    assert_eq!(
        call(&client, &base, "GET", &missing, None, "default")
            .await
            .status(),
        404
    );
    denied_requests(f, &client, &base, &input).await;
    mcp(f, &client, &base).await;
    sdks(f, &base).await;
    assert_eq!(f.count("subscription_dependency_claims").await, 0);
    uncertain(f, &client, &base).await;
    assert_eq!(f.count("subscription_dependency_claims").await, 4);
    server.abort();
    let _ = server.await;
}
async fn denied_requests(f: &Harness, client: &reqwest::Client, base: &str, input: &Value) {
    let before = f.provider.requests.load(Ordering::SeqCst);
    let posts = f.posts();
    for (key, value) in [
        ("operationId", json!(Uuid::nil().to_string())),
        ("quantity", json!(0)),
        ("quantity", json!(1.5)),
        ("quantity", json!(9_007_199_254_740_992_u64)),
        ("localCustomerId", json!(Uuid::nil())),
        ("localPlanId", json!(Uuid::nil())),
        ("description", json!("x".repeat(501))),
        ("description", json!("bad\0value")),
        ("id", json!("caller")),
        ("stripeFields", json!({})),
    ] {
        let mut bad = body(f);
        bad[key] = value;
        assert_eq!(
            call(
                client,
                base,
                "POST",
                "/bound-subscriptions",
                Some(&bad),
                "default"
            )
            .await
            .status(),
            400,
            "{key}"
        );
    }
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    let mut changed = input.clone();
    changed["quantity"] = json!(3);
    assert_eq!(
        call(
            client,
            base,
            "POST",
            "/bound-subscriptions",
            Some(&changed),
            "default"
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        call(
            client,
            base,
            "POST",
            "/bound-subscriptions",
            Some(input),
            "other"
        )
        .await
        .status(),
        409
    );
    assert_eq!(f.posts(), posts);
}
async fn uncertain(f: &Harness, client: &reqwest::Client, base: &str) {
    *f.provider.readback.lock().unwrap() =
        Some(json!({"id":"bad","object":"customer","livemode":false}));
    let input = body(f);
    let before = f.posts();
    let result = call(
        client,
        base,
        "POST",
        "/bound-subscriptions",
        Some(&input),
        "default",
    )
    .await;
    assert_eq!(result.status(), 202);
    let result: Value = result.json().await.unwrap();
    assert_eq!(result["state"], "unresolved");
    assert_eq!(result.as_object().unwrap().len(), 2);
    assert_eq!(
        call(
            client,
            base,
            "POST",
            "/bound-subscriptions",
            Some(&input),
            "default"
        )
        .await
        .status(),
        202
    );
    assert_eq!(f.posts(), before + 1);
    let inspect = format!(
        "/bound-subscription-operations/{}",
        input["operationId"].as_str().unwrap()
    );
    let value = call(client, base, "GET", &inspect, None, "default")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(value["state"], "pending");
    assert_eq!(value.as_object().unwrap().len(), 2);
}
async fn mcp(f: &Harness, client: &reqwest::Client, base: &str) {
    let input = body(f);
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"createBoundSubscription","arguments":{"body":input,"headers":{"authorization":sign("POST","/bound-subscriptions",Some(&input),"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
    let path = format!(
        "/bound-subscription-operations/{}",
        input["operationId"].as_str().unwrap()
    );
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspectBoundSubscriptionOperation","arguments":{"params":{"operationId":input["operationId"]},"headers":{"authorization":sign("GET",&path,None,"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
}
async fn startup(f: &Harness) {
    let before = f.provider.requests.load(Ordering::SeqCst);
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
        ("records", "SELECT"),
        ("records", "UPDATE"),
        ("provider_bindings", "SELECT"),
        ("provider_bindings", "UPDATE"),
        ("stripe_customer_bindings", "SELECT"),
        ("stripe_customer_bindings", "UPDATE"),
        ("plan_creation_intents", "SELECT"),
        ("sensitive_read_audit", "INSERT"),
        ("subscription_creation_intents", "SELECT"),
        ("subscription_creation_intents", "INSERT"),
        ("subscription_dependency_claims", "INSERT"),
        ("subscription_creation_audit", "INSERT"),
    ] {
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
                        Some(f.publisher().await)
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
    publisher_denials(f).await;
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
}
async fn publisher_denials(f: &Harness) {
    assert!(BoundBillingV1::trusted(config(f), None, None).is_err());
    assert!(
        f.store
            .trusted_subscription_publisher(f.pool.clone())
            .await
            .is_err()
    );
    let publisher = f.publisher().await;
    assert!(
        publisher
            .with_audit_context(forklaunch_core::domain::AuditContext::actor(
                " ",
                "bad-actor"
            ))
            .is_err()
    );
    let other = publisher
        .with_audit_context(forklaunch_core::domain::AuditContext::actor(
            "other",
            "wrong-actor",
        ))
        .unwrap();
    assert!(other.validate_creation_access(&f.store).await.is_err());
    for (table, privilege) in [
        ("records", "SELECT"),
        ("records", "UPDATE"),
        ("provider_bindings", "SELECT"),
        ("provider_bindings", "UPDATE"),
        ("stripe_customer_bindings", "SELECT"),
        ("stripe_customer_bindings", "UPDATE"),
        ("plan_creation_intents", "SELECT"),
        ("sensitive_read_audit", "INSERT"),
    ] {
        let publisher = f.publisher().await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {}",
            f.publisher_role
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
            f.publisher_role
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    let publisher = f.publisher().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) FROM {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    assert!(
        BillingBackend::stripe((*f.store).clone(), f.client.clone())
            .with_bound_billing(
                BoundBillingV1::trusted_with_subscription(config(f), None, None, Some(publisher))
                    .unwrap()
            )
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role))).execute(&f.owner).await.unwrap();
}
async fn sdks(f: &Harness, base: &str) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_subscription_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-subscription-creation"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-subscription-creation")
            .with_client_struct("BoundSubscriptionClient"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!(
        "fl-bound-subscription-creation-sdk-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_subscription_creation_route_sdks.mjs");
    let base = base.to_owned();
    let customer = f.customer.to_string();
    let plan = f.plan.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts_path)
            .arg(rs_path)
            .arg(base)
            .arg(customer)
            .arg(plan)
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
