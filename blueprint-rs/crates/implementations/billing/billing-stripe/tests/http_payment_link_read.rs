#![cfg(feature = "bound-checkout")]
#[path = "support/bound_creation_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fixture::{Fixture, request};
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1,
    bound_payment_link_read_contracts,
};
use forklaunch_billing_stripe::{
    bound_creation::BoundPaymentLinkCreationOutcome, client::StripeBillingClient,
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
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned-deployment","accountId":"acct_owned","livemode":false,"paymentLinkRead":{"trustedPrices":["price_owned"]}}).to_string()).unwrap()
}
fn sign(path: &str, key_id: &str) -> String {
    generate_hmac_auth_header(
        KEY,
        key_id,
        "GET",
        path.split('?').next().unwrap(),
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
    for route in bound_payment_link_read_contracts() {
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
async fn bound_payment_link_read_http_mcp_sdk() {
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
async fn matrix(f: &Fixture) {
    let store = f
        .store
        .with_audit_context(AuditContext::actor("service:default", "route-fixture"))
        .unwrap();
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store.clone())),
        f.client.clone(),
    );
    let BoundPaymentLinkCreationOutcome::Completed { record, .. } = service
        .create_bound_payment_link(&f.profile, Uuid::new_v4(), request())
        .await
        .unwrap()
    else {
        panic!("created PaymentLink")
    };
    let id = record.id.clone();
    // From now on runtime can only read these tables and append sensitive-read audits.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE INSERT,UPDATE,DELETE ON fl_billing.records,fl_billing.provider_bindings,fl_billing.provider_binding_audit,fl_billing.payment_link_creation_intents,fl_billing.payment_link_creation_audit FROM {}",f.role))).execute(&f.owner).await.unwrap();
    startup(f).await;
    let mut fresh = record.provider_fields.clone().unwrap();
    fresh["active"] = json!(false);
    fresh["metadata"] = json!({"presentation":"fresh"});
    *f.provider.readback.lock().unwrap() = Some(fresh);
    let backend = BillingBackend::stripe(store, f.client.clone())
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
    let path = format!("/bound-payment-links/{id}");
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
    assert_eq!(value["amount"], record.amount);
    assert_eq!(value["status"], record.status);
    assert_eq!(value["stripeFields"]["active"], false);
    assert_eq!(value["stripeFields"]["metadata"]["presentation"], "fresh");
    assert!(value.get("providerFields").is_none());
    get_denials(f, &client, &base, &path).await;
    let mcp=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"getBoundCreatedPaymentLink","arguments":{"params":{"publicId":id},"headers":{"authorization":sign(&path,"default")}}}})).send().await.unwrap();
    let mcp: Value = mcp.json().await.unwrap();
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    list_routes(f, &client, &base, &id).await;
    sdks(&base, &id).await;
    *f.provider.readback.lock().unwrap() =
        Some(json!({"id":"plink_wrong","object":"payment_link","livemode":false}));
    assert_eq!(get(&client, &base, &path, "default").await.status(), 409);
    assert_eq!(f.posts(), before);
    server.abort();
    let _ = server.await;
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
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE USAGE ON SCHEMA fl_billing FROM {}",
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
        "GRANT USAGE ON SCHEMA fl_billing TO {}",
        f.role
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    for (table, privilege) in [
        ("records", "SELECT"),
        ("provider_bindings", "SELECT"),
        ("payment_link_creation_intents", "SELECT"),
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
    for route in bound_payment_link_read_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-payment-link-read"),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-payment-link-read")
            .with_client_struct("BoundPaymentLinkClient"),
    )
    .unwrap();
    let root =
        std::env::temp_dir().join(format!("fl-bound-payment-link-read-sdk-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../../../framework-rs/parity/rust_billing_bound_payment_link_read_route_sdks.mjs",
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

async fn list_routes(f: &Fixture, client: &reqwest::Client, base: &str, id: &str) {
    let query = |ids: Value| {
        format!(
            "/bound-payment-links?{}",
            form_urlencoded::Serializer::new(String::new())
                .append_pair("ids", &ids.to_string())
                .finish()
        )
    };
    let path = query(json!([id, id]));
    assert_eq!(
        client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = get(client, base, &path, "default").await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.json::<Value>().await.unwrap(), json!([]));
    f.provider.readback.lock().unwrap().as_mut().unwrap()["active"] = json!(true);
    let response = get(client, base, &path, "default").await;
    assert_eq!(response.status(), 200);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response.as_array().unwrap().len(), 2);
    assert_eq!(response[0]["id"], id);
    assert_eq!(response[1]["id"], id);
    assert_eq!(response[0]["stripeFields"]["active"], true);
    let before = f.provider.requests.load(Ordering::SeqCst);
    for path in [
        "/bound-payment-links".into(),
        query(json!([])),
        query(json!(["invalid"])),
        query(json!([Uuid::nil().to_string()])),
        query(json!(vec![id; 101])),
        format!("{path}&extra=1"),
        format!("{path}&ids=[]"),
    ] {
        assert_eq!(
            get(client, base, &path, "default").await.status(),
            400,
            "{path}"
        );
    }
    assert_eq!(get(client, base, &path, "other").await.status(), 409);
    assert_eq!(f.provider.requests.load(Ordering::SeqCst), before);
    let mcp=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"listBoundCreatedPaymentLinks","arguments":{"query":{"ids":[id,id]},"headers":{"authorization":sign(&path,"default")}}}})).send().await.unwrap();
    let mcp: Value = mcp.json().await.unwrap();
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
}

async fn get_denials(f: &Fixture, client: &reqwest::Client, base: &str, path: &str) {
    let before_denials = f.provider.requests.load(Ordering::SeqCst);
    assert_eq!(get(client, base, path, "other").await.status(), 409);
    for invalid in ["invalid", "00000000-0000-0000-0000-000000000000"] {
        assert_eq!(
            get(
                client,
                base,
                &format!("/bound-payment-links/{invalid}"),
                "default"
            )
            .await
            .status(),
            400
        );
    }
    assert_eq!(
        get(
            client,
            base,
            &format!("/bound-payment-links/{}", Uuid::new_v4()),
            "default"
        )
        .await
        .status(),
        409
    );
    assert_eq!(before_denials, f.provider.requests.load(Ordering::SeqCst));
}
