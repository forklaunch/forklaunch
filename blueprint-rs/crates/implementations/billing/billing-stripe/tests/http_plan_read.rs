#![cfg(feature = "bound-checkout")]
#[path = "support/bound_plan_creation_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::{Fixture, plan, request};
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1, bound_plan_read_contracts,
};
use forklaunch_billing_stripe::{
    bound_plan_creation::BoundPlanCreationOutcome, client::StripeBillingClient,
    services::StripeBillingService,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::domain::AuditContext;
use jsonwebtoken::{Algorithm, Validation};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;
const KEY: &[u8] = b"billing-date-fixture";
fn config() -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned-deployment","accountId":"acct_owned","livemode":false,"planRead":{"trustedProducts":["custom-product"]}}).to_string()).unwrap()
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
    for route in bound_plan_read_contracts() {
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
async fn bound_plan_read_http_mcp_sdk() {
    let f = Fixture::new().await;
    let store = f
        .store
        .with_audit_context(AuditContext::actor("service:default", "route-fixture"))
        .unwrap();
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store.clone())),
        f.client.clone(),
    );
    let BoundPlanCreationOutcome::Completed { record, .. } = service
        .create_bound_plan(&f.profile, Uuid::new_v4(), request())
        .await
        .unwrap()
    else {
        panic!("created Plan")
    };
    let id = record.id.clone();
    // From now on runtime can only read these tables and append sensitive-read audits.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings,fl_billing.provider_binding_audit,fl_billing.plan_creation_intents,fl_billing.plan_creation_audit FROM {}",f.role))).execute(&f.owner).await.unwrap();
    startup(&f).await;
    let mut fresh = plan(&record.external_id);
    fresh["product"]["name"] = json!("Fresh product");
    *f.provider.readback.lock().unwrap() = Some(fresh);
    let backend = BillingBackend::stripe(store, f.client.clone())
        .with_bound_billing(BoundBillingV1::trusted(config(), None, None).unwrap())
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
    let path = format!("/bound-plans/{id}");
    let before = f.posts();
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
    assert_eq!(value["name"], record.name);
    assert_eq!(value["price"], record.price);
    assert_eq!(value["stripeFields"]["name"], "Fresh product");
    assert!(value.get("providerFields").is_none());
    assert_eq!(get(&client, &base, &path, "other").await.status(), 409);
    for invalid in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        assert_eq!(
            get(
                &client,
                &base,
                &format!("/bound-plans/{invalid}"),
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
            &format!("/bound-plans/{}", Uuid::new_v4()),
            "default"
        )
        .await
        .status(),
        409
    );
    let mcp=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"getBoundCreatedPlan","arguments":{"params":{"publicId":id},"headers":{"authorization":sign(&path,"default")}}}})).send().await.unwrap();
    let mcp: Value = mcp.json().await.unwrap();
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    sdks(&base, &id).await;
    *f.provider.readback.lock().unwrap() = Some(plan("wrong-provider-id"));
    assert_eq!(get(&client, &base, &path, "default").await.status(), 409);
    assert_eq!(f.posts(), before);
    server.abort();
    let _ = server.await;
    f.cleanup().await;
}
async fn get(client: &reqwest::Client, base: &str, path: &str, actor: &str) -> reqwest::Response {
    client
        .get(format!("{base}{path}"))
        .header("authorization", sign(path, actor))
        .send()
        .await
        .unwrap()
}
async fn startup(f: &Fixture) {
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
                .with_bound_billing(BoundBillingV1::trusted(config(), None, None).unwrap())
                .await
                .is_err()
        );
    }
    for (table, privilege) in [
        ("records", "SELECT"),
        ("provider_bindings", "SELECT"),
        ("plan_creation_intents", "SELECT"),
        ("sensitive_read_audit", "INSERT"),
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
                .with_bound_billing(BoundBillingV1::trusted(config(), None, None).unwrap())
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
    assert_eq!(before, f.provider.requests.load(Ordering::SeqCst));
}
async fn sdks(base: &str, id: &str) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_plan_read_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-plan-read"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-plan-read")
            .with_client_struct("BoundPlanClient"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!("fl-bound-plan-read-sdk-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_plan_read_route_sdks.mjs");
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
