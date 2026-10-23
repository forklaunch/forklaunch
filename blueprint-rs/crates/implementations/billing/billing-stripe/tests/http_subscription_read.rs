#![cfg(feature = "bound-checkout")]
#[path = "support/bound_subscription_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::Fixture;
#[path = "support/subscription_read_provider.rs"]
mod provider;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1,
    bound_subscription_read_contracts,
};
use forklaunch_billing_stripe::{
    bound_subscription_creation::{
        BoundSubscriptionCreationOutcome, BoundSubscriptionCreationProfile,
        SubscriptionCollectionPolicy,
    },
    client::StripeBillingClient,
    services::StripeBillingService,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::domain::AuditContext;
use jsonwebtoken::{Algorithm, Validation};
use provider::{request, subscription};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;
const KEY: &[u8] = b"billing-date-fixture";
fn config(f: &Fixture) -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"subscriptionRead":{"plans":[{"localPlanId":f.plan,"creationOperationId":f.plan_operation}],"collectionPolicy":"automatic_default_incomplete_v1"}}).to_string()).unwrap()
}
fn sign(path: &str, key_id: &str) -> String {
    generate_hmac_auth_header(
        KEY,
        key_id,
        "GET",
        path,
        SignedBody::Absent,
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
    for route in bound_subscription_read_contracts() {
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
async fn bound_subscription_read_http_mcp_sdk() {
    let fixture = Arc::new(Fixture::new().await);
    let run = fixture.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(fixture).ok().unwrap().cleanup().await;
    result.unwrap();
}
async fn matrix(f: &Fixture) {
    let (p, provider_client, provider_server) = provider::start().await;
    let (store, record, publisher_pool, role) = create_fixture(f, &provider_client).await;
    let id = record.id.clone();
    startup(f, &provider_client, &p, &role).await;
    let mut fresh = subscription(1);
    fresh["status"] = json!("active");
    fresh["metadata"] = json!({"presentation":"fresh"});
    *p.readback.lock().unwrap() = Some(fresh);
    let backend = BillingBackend::stripe(store, provider_client.clone())
        .with_bound_billing(BoundBillingV1::trusted(config(f), None, None).unwrap())
        .await
        .unwrap();
    let router =
        BillingHttp::new(backend, authentication(), ServiceInfo::new("billing", "1")).router();
    let legacy = forklaunch_billing_interface::module_contracts::module_registry(true);
    assert_eq!(router.registry().routes().len(), legacy.routes().len() + 1);
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
    let path = format!("/bound-subscriptions/{id}");
    let before = p.posts.load(Ordering::SeqCst);
    assert_eq!(
        client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let value = get(&client, &base, &path, "default").await;
    assert_eq!(value.status(), 200);
    let value: Value = value.json().await.unwrap();
    assert_eq!(value["id"], id);
    assert_eq!(value["status"], record.status);
    assert_eq!(value["active"], record.active);
    assert_eq!(value["partyId"], record.party_id);
    assert_eq!(value["productId"], record.product_id);
    assert_eq!(value["stripeFields"]["metadata"]["presentation"], "fresh");
    assert!(value.get("providerFields").is_none());
    let before_denials = p.requests.load(Ordering::SeqCst);
    assert_eq!(get(&client, &base, &path, "other").await.status(), 409);
    for invalid in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        assert_eq!(
            get(
                &client,
                &base,
                &format!("/bound-subscriptions/{invalid}"),
                "default"
            )
            .await
            .status(),
            400
        );
    }
    assert_eq!(
        get(
            &client,
            &base,
            &format!("/bound-subscriptions/{}", Uuid::new_v4()),
            "default"
        )
        .await
        .status(),
        409
    );
    assert_eq!(p.requests.load(Ordering::SeqCst), before_denials);
    let mcp=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"getBoundCreatedSubscription","arguments":{"params":{"publicId":id},"headers":{"authorization":sign(&path,"default")}}}})).send().await.unwrap();
    let mcp: Value = mcp.json().await.unwrap();
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    sdks(&base, &id).await;
    let mut wrong = subscription(1);
    wrong["id"] = json!("sub_wrong");
    *p.readback.lock().unwrap() = Some(wrong);
    assert_eq!(get(&client, &base, &path, "default").await.status(), 409);
    assert_eq!(p.posts.load(Ordering::SeqCst), before);
    server.abort();
    let _ = server.await;
    provider_server.abort();
    let _ = provider_server.await;
    publisher_pool.close().await;
}
async fn create_fixture(
    f: &Fixture,
    provider_client: &StripeBillingClient,
) -> (
    forklaunch_billing_base::services::postgres::PostgresBillingStore,
    forklaunch_billing_interface::services::SubscriptionDto,
    sqlx::PgPool,
    String,
) {
    let store = f
        .store
        .with_audit_context(AuditContext::actor("service:default", "route-fixture"))
        .unwrap();
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store.clone())),
        provider_client.clone(),
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let publisher = store
        .trusted_subscription_publisher(publisher_pool.clone())
        .await
        .unwrap();
    let profile = BoundSubscriptionCreationProfile::trusted(
        f.scope.clone(),
        vec![(f.plan, f.plan_operation)],
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete,
    )
    .unwrap();
    let BoundSubscriptionCreationOutcome::Completed { record, .. } = service
        .create_bound_subscription(&profile, &publisher, Uuid::new_v4(), request(f))
        .await
        .unwrap()
    else {
        panic!("created Subscription")
    };
    let role: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    // From now on runtime can only read these tables and append sensitive-read audits.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings,fl_billing.provider_binding_audit,fl_billing.plan_creation_intents,fl_billing.plan_creation_audit,fl_billing.stripe_customer_bindings,fl_billing.subscription_creation_intents,fl_billing.subscription_creation_audit,fl_billing.subscription_dependency_claims FROM {role}"))).execute(&f.owner).await.unwrap();
    (store, *record, publisher_pool, role)
}
async fn get(client: &reqwest::Client, base: &str, path: &str, actor: &str) -> reqwest::Response {
    client
        .get(format!("{base}{path}"))
        .header("authorization", sign(path, actor))
        .send()
        .await
        .unwrap()
}
async fn startup(
    f: &Fixture,
    provider_client: &StripeBillingClient,
    p: &provider::Provider,
    role: &str,
) {
    let before = p.requests.load(Ordering::SeqCst);
    for backend in [
        BillingBackend::base((*f.store).clone()),
        BillingBackend::stripe((*f.store).clone(), provider_client.clone())
            .with_database_backup(true),
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
                .with_bound_billing(BoundBillingV1::trusted(config(f), None, None).unwrap())
                .await
                .is_err()
        );
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE USAGE ON SCHEMA fl_billing FROM {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert!(
        BillingBackend::stripe((*f.store).clone(), provider_client.clone())
            .with_bound_billing(BoundBillingV1::trusted(config(f), None, None).unwrap())
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT USAGE ON SCHEMA fl_billing TO {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    for (table, privilege) in [
        ("records", "SELECT"),
        ("provider_bindings", "SELECT"),
        ("plan_creation_intents", "SELECT"),
        ("subscription_creation_intents", "SELECT"),
        ("stripe_customer_bindings", "SELECT"),
        ("sensitive_read_audit", "INSERT"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        assert!(
            BillingBackend::stripe((*f.store).clone(), provider_client.clone())
                .with_bound_billing(BoundBillingV1::trusted(config(f), None, None).unwrap())
                .await
                .is_err()
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT {privilege} ON fl_billing.{table} TO {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    assert_eq!(before, p.requests.load(Ordering::SeqCst));
}
async fn sdks(base: &str, id: &str) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_subscription_read_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-subscription-read"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-subscription-read")
            .with_client_struct("BoundSubscriptionClient"),
    )
    .unwrap();
    let root =
        std::env::temp_dir().join(format!("fl-bound-subscription-read-sdk-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../../../framework-rs/parity/rust_billing_bound_subscription_read_route_sdks.mjs",
    );
    let base = base.to_owned();
    let id = id.to_owned();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts_path)
            .arg(rs_path)
            .arg(base)
            .arg(id)
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
