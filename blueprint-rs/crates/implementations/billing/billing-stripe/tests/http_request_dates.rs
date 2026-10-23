#![cfg(feature = "http")]
use axum::{Json, Router, routing::post};
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::postgres::PostgresBillingStore;
use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_billing_stripe::http::{BillingBackend, BillingHttp, BillingRequestDateProfile};
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_core::{
    ServiceInfo,
    persistence::{TenantContext, encryption::FieldEncryptor},
};
use jsonwebtoken::{Algorithm, Validation};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
const SECRET: &[u8] = b"billing-date-fixture";
fn sign(method: &str, path: &str, body: Option<&str>) -> String {
    generate_hmac_auth_header(
        SECRET,
        "default",
        method,
        path,
        body.map_or(SignedBody::Absent, SignedBody::Text),
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap()
}
async fn call(
    client: &Client,
    base: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> reqwest::Response {
    let body = body.map(|body| body.to_string());
    let mut request = client
        .request(method.clone(), format!("{base}{path}"))
        .header(
            "authorization",
            sign(
                method.as_str(),
                path.split('?').next().unwrap(),
                body.as_deref(),
            ),
        );
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(body);
    }
    request.send().await.unwrap()
}
async fn value(response: reqwest::Response) -> Value {
    assert!(
        response.status().is_success(),
        "HTTP {}: {}",
        response.status(),
        response.text().await.unwrap()
    );
    response.json().await.unwrap()
}
async fn rpc(client: &Client, base: &str, method: &str, params: Value) -> Value {
    let response = client
        .post(format!("{base}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    response.json().await.unwrap()
}
async fn serve(
    router: Router,
) -> (
    String,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (base, stop, task)
}

#[tokio::test]
#[ignore = "owned isolated PostgreSQL database; loopback HTTP/MCP/SDK only"]
async fn configured_date_http_mcp_sdk() {
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap())
        .await
        .unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let database = format!("fl_dates_{suffix}");
    let role = format!("fl_dates_runtime_{suffix}");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE ROLE {role} LOGIN NOINHERIT NOSUPERUSER NOBYPASSRLS PASSWORD 'owned-date-fixture'"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
        .execute(&admin)
        .await
        .unwrap();
    let owner = PgPoolOptions::new()
        .connect_with((*admin.connect_options()).clone().database(&database))
        .await
        .unwrap();
    PostgresBillingStore::migrate(&owner).await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {role};GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {role}"))).execute(&owner).await.unwrap();
    let pool = PgPoolOptions::new()
        .connect_with(
            (*owner.connect_options())
                .clone()
                .username(&role)
                .password("owned-date-fixture"),
        )
        .await
        .unwrap();
    let run = pool.clone();
    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move { Box::pin(matrix(&run)).await }).await
        })
        .await;
    pool.close().await;
    owner.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE {database} WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {role}")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    result.unwrap();
}
fn body(stripe: bool, date: &Value) -> Value {
    json!({"partyId":"cus_owned","partyType":"user","productId":"plan_owned","active":true,"externalId":uuid::Uuid::new_v4().to_string(),"billingProvider":if stripe{"stripe"}else{"none"},"startDate":date,"status":"PENDING"})
}
#[allow(clippy::too_many_lines)]
async fn matrix(pool: &PgPool) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let provider = Router::new().route(
        "/v1/subscriptions",
        post(move || {
            let calls = counted.clone();
            async move {
                let id = calls.fetch_add(1, Ordering::SeqCst);
                Json(json!({"id":format!("sub_owned_{id}"),"object":"subscription"}))
            }
        }),
    );
    let (provider_base, provider_stop, provider_task) = serve(provider).await;
    let client = Client::new();
    for stripe in [false, true] {
        for profile in [
            BillingRequestDateProfile::Legacy,
            BillingRequestDateProfile::UtcIsoAndEpoch,
        ] {
            let store = PostgresBillingStore::new(
                pool.clone(),
                TenantContext::new(format!("dates-{stripe}-{profile:?}")).unwrap(),
                Arc::new(FieldEncryptor::new("owned-date-key").unwrap()),
                AuditContext::system("dates"),
            )
            .await
            .unwrap();
            let backend = if stripe {
                BillingBackend::stripe(
                    store,
                    StripeBillingClient::with_transport(
                        "owned".into(),
                        ProviderHttpClient::new(&provider_base, Duration::from_secs(5), 1_048_576)
                            .unwrap(),
                    )
                    .unwrap(),
                )
            } else {
                BillingBackend::base(store)
            }
            .with_request_dates(profile);
            let auth = HmacJwksAuth::new(
                SECRET.to_vec(),
                RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
                Validation::new(Algorithm::RS256),
            )
            .unwrap();
            let router =
                BillingHttp::new(backend, auth, ServiceInfo::new("billing", "1.0.0")).router();
            let route = router
                .registry()
                .routes()
                .iter()
                .find(|r| r.operation_name == "createSubscription")
                .unwrap()
                .clone();
            let schema: Value = serde_json::from_str(
                route
                    .request
                    .body
                    .as_ref()
                    .unwrap()
                    .schema
                    .json_schema
                    .as_ref()
                    .unwrap(),
            )
            .unwrap();
            let response_before =
                forklaunch_billing_interface::module_contracts::module_registry(stripe)
                    .routes()
                    .iter()
                    .find(|r| r.operation_name == "createSubscription")
                    .unwrap()
                    .responses
                    .clone();
            assert_eq!(route.responses, response_before);
            let sdk = forklaunch_core::typescript::generate_typescript_http_sdk_source(
                &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
                    forklaunch_core::codegen::SdkTarget::TypeScript,
                    router.registry(),
                ),
                &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("billing"),
            )
            .unwrap();
            let (base, stop, task) = serve(router.into_router_with_default_mcp().unwrap()).await;
            if profile == BillingRequestDateProfile::Legacy {
                let before = calls.load(Ordering::SeqCst);
                assert_eq!(
                    call(
                        &client,
                        &base,
                        Method::POST,
                        "/subscription",
                        Some(body(stripe, &json!(0)))
                    )
                    .await
                    .status(),
                    400
                );
                assert_eq!(calls.load(Ordering::SeqCst), before);
            } else {
                assert!(schema["properties"]["startDate"].get("anyOf").is_some());
                for date in [
                    json!(0),
                    json!(1.9),
                    json!(-1.9),
                    json!("2026-10-14"),
                    json!("2026-10-14T01:02:03.123456Z"),
                ] {
                    let mut input = body(stripe, &date);
                    let result = value(
                        call(
                            &client,
                            &base,
                            Method::POST,
                            "/subscription",
                            Some(input.clone()),
                        )
                        .await,
                    )
                    .await;
                    assert!(result["startDate"].is_string());
                    input["externalId"] = json!(uuid::Uuid::new_v4().to_string());
                    let raw = input.to_string();
                    let result=rpc(&client,&base,"tools/call",json!({"name":"createSubscription","arguments":{"body":input,"headers":{"authorization":sign("POST","/subscription",Some(&raw))}}})).await;
                    assert_eq!(result["result"]["isError"], false, "{result}");
                }
                for date in [
                    Value::Null,
                    json!(true),
                    json!({}),
                    json!("invalid"),
                    json!("2026-10-14T01:02:03"),
                    json!(8_640_000_000_000_000_i64),
                    json!(8_640_000_000_000_001_i64),
                ] {
                    let input = body(stripe, &date);
                    let before = calls.load(Ordering::SeqCst);
                    assert_eq!(
                        call(
                            &client,
                            &base,
                            Method::POST,
                            "/subscription",
                            Some(input.clone())
                        )
                        .await
                        .status(),
                        400
                    );
                    let raw = input.to_string();
                    let result=rpc(&client,&base,"tools/call",json!({"name":"createSubscription","arguments":{"body":input,"headers":{"authorization":sign("POST","/subscription",Some(&raw))}}})).await;
                    assert!(
                        result.get("error").is_some() || result["result"]["isError"] == true,
                        "{result}"
                    );
                    assert_eq!(calls.load(Ordering::SeqCst), before);
                }
                let file =
                    std::env::temp_dir().join(format!("fl-dates-sdk-{}.ts", uuid::Uuid::new_v4()));
                std::fs::write(&file, sdk).unwrap();
                let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../../../../framework-rs/parity/rust_billing_request_dates_sdk.mjs");
                let output = tokio::task::spawn_blocking(move || {
                    let result = std::process::Command::new("node")
                        .arg(script)
                        .arg(&file)
                        .arg(base)
                        .arg(if stripe { "stripe" } else { "base" })
                        .output()
                        .unwrap();
                    std::fs::remove_file(file).unwrap();
                    result
                })
                .await
                .unwrap();
                assert!(
                    output.status.success(),
                    "SDK: {} {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
            stop.send(()).unwrap();
            task.await.unwrap();
        }
    }
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
}
