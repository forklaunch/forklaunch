#![cfg(feature = "bound-checkout")]
#[path = "support/bound_plan_creation_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::Fixture;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1, bound_plan_contracts,
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
fn config() -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned-deployment","accountId":"acct_owned","livemode":false,"plan":{"trustedProducts":["custom-product"]}}).to_string()).unwrap()
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
    for route in bound_plan_contracts() {
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
async fn bound_plan_creation_http_mcp_sdk() {
    let f = Arc::new(Fixture::new().await);
    let run = f.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    Arc::try_unwrap(f).ok().unwrap().cleanup().await;
    result.unwrap();
}
fn body() -> Value {
    json!({"operationId":Uuid::new_v4().to_string(),"name":"Owned plan","unitAmount":1200,"currency":"USD","cadence":"month","productId":"custom-product","active":true})
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
async fn matrix(f: &Fixture) {
    startup(f).await;
    // Creation authority does not require records/binding reads or sensitive-read audits.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE SELECT ON fl_billing.records,fl_billing.provider_bindings FROM {}; REVOKE INSERT ON fl_billing.sensitive_read_audit FROM {}",f.role,f.role))).execute(&f.owner).await.unwrap();
    let backend = BillingBackend::stripe((*f.store).clone(), f.client.clone())
        .with_bound_billing(BoundBillingV1::trusted(config(), None, None).unwrap())
        .await
        .unwrap();
    let router =
        BillingHttp::new(backend, authentication(), ServiceInfo::new("billing", "1")).router();
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
    let input = body();
    let path = "/bound-plans";
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
    assert_ne!(completed["publicId"], "custom-plan-1");
    let again = call(&client, &base, "POST", path, Some(&input), "default")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(again, completed);
    let mut integral_float = input.clone();
    integral_float["unitAmount"] = json!(1200.0);
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
        "/bound-plan-operations/{}",
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
    let missing = format!("/bound-plan-operations/{}", Uuid::new_v4());
    assert_eq!(
        call(&client, &base, "GET", &missing, None, "default")
            .await
            .status(),
        404
    );
    denied_requests(f, &client, &base, &input).await;
    mcp(&client, &base).await;
    sdks(&base).await;
    uncertain(f, &client, &base).await;
    server.abort();
    let _ = server.await;
}
async fn denied_requests(f: &Fixture, client: &reqwest::Client, base: &str, input: &Value) {
    let before = f.provider.requests.load(Ordering::SeqCst);
    let posts = f.posts();
    for (key, value) in [
        ("operationId", json!(Uuid::nil().to_string())),
        ("unitAmount", json!(-1)),
        ("unitAmount", json!(1.5)),
        ("unitAmount", json!(9_007_199_254_740_992_u64)),
        ("currency", json!("usd")),
        ("name", json!(" ")),
        ("description", json!("x".repeat(10001))),
        ("features", json!(vec!["x"; 1001])),
        ("cadence", json!("fortnight")),
        ("productId", json!("untrusted-product")),
        ("id", json!("caller")),
        ("stripeFields", json!({})),
    ] {
        let mut bad = body();
        bad[key] = value;
        assert_eq!(
            call(client, base, "POST", "/bound-plans", Some(&bad), "default")
                .await
                .status(),
            400,
            "{key}"
        );
    }
    let mut changed = input.clone();
    changed["unitAmount"] = json!(1300);
    assert_eq!(
        call(
            client,
            base,
            "POST",
            "/bound-plans",
            Some(&changed),
            "default"
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        call(client, base, "POST", "/bound-plans", Some(input), "other")
            .await
            .status(),
        409
    );
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    assert_eq!(f.posts(), posts);
}
async fn uncertain(f: &Fixture, client: &reqwest::Client, base: &str) {
    *f.provider.response.lock().unwrap() =
        Some(json!({"id":"bad","object":"customer","livemode":false}));
    let input = body();
    let before = f.posts();
    let result = call(
        client,
        base,
        "POST",
        "/bound-plans",
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
            "/bound-plans",
            Some(&input),
            "default"
        )
        .await
        .status(),
        202
    );
    assert_eq!(f.posts(), before + 1);
    let inspect = format!(
        "/bound-plan-operations/{}",
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
async fn mcp(client: &reqwest::Client, base: &str) {
    let input = body();
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"createBoundPlan","arguments":{"body":input,"headers":{"authorization":sign("POST","/bound-plans",Some(&input),"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
    let path = format!(
        "/bound-plan-operations/{}",
        input["operationId"].as_str().unwrap()
    );
    let result=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspectBoundPlanOperation","arguments":{"params":{"operationId":input["operationId"]},"headers":{"authorization":sign("GET",&path,None,"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(result["result"]["isError"], false, "{result}");
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
        ("records", "INSERT"),
        ("provider_bindings", "INSERT"),
        ("plan_creation_intents", "SELECT"),
        ("plan_creation_intents", "INSERT"),
        ("plan_creation_intents", "UPDATE"),
        ("plan_creation_audit", "INSERT"),
        ("provider_binding_audit", "INSERT"),
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
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
}
async fn sdks(base: &str) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_plan_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-plan-creation"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-plan-creation")
            .with_client_struct("BoundPlanClient"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!("fl-bound-plan-creation-sdk-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_plan_creation_route_sdks.mjs");
    let base = base.to_owned();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts_path)
            .arg(rs_path)
            .arg(base)
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
