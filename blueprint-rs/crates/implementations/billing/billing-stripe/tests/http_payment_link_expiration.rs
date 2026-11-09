#![cfg(feature = "bound-checkout")]
#[path = "support/bound_portal_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::get,
};
use fixture::Fixture;
use forklaunch_billing_base::services::postgres::PaymentLinkCreationClaim;
use forklaunch_billing_stripe::client::{STRIPE_API_VERSION, StripeBillingClient};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;

use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1,
    bound_payment_link_expiration_contracts,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use jsonwebtoken::{Algorithm, Validation};
const KEY: &[u8] = b"billing-date-fixture";
#[derive(Clone, Default)]
struct Wire {
    posts: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    invalid: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicBool>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
    arm: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}
fn headers(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer expiration-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
async fn post(
    State(w): State<Wire>,
    Path(id): Path<String>,
    h: HeaderMap,
    body: String,
) -> Json<Value> {
    headers(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-plink-expire-")
    );
    assert_eq!(body, "metadata%5Bstatus%5D=EXPIRED");
    w.posts.fetch_add(1, Ordering::SeqCst);
    if w.pause.load(Ordering::SeqCst) {
        w.entered.notify_one();
        w.resume.notified().await;
    }
    if let Some(arm) = w.arm.lock().unwrap().as_ref() {
        arm.store(true, Ordering::SeqCst);
    }
    Json(w.invalid.lock().unwrap().clone().unwrap_or_else(||json!({"id":id,"object":"payment_link","livemode":false,"active":true,"metadata":{"status":"EXPIRED"}})))
}
async fn server(w: Wire) -> (StripeBillingClient, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route(
            "/v1/account",
            get(|h: HeaderMap| async move {
                headers(&h);
                Json(json!({"id":"acct_owned","object":"account"}))
            }),
        )
        .route(
            "/v1/balance",
            get(|h: HeaderMap| async move {
                headers(&h);
                Json(json!({"object":"balance","livemode":false}))
            }),
        )
        .route(
            "/v1/payment_links/{id}",
            get(|Path(id): Path<String>, h: HeaderMap| async move {
                headers(&h);
                Json(json!({"id":id,"object":"payment_link","livemode":false,"active":true}))
            })
            .post(post),
        )
        .with_state(w.clone())
        .layer(axum::middleware::from_fn(
            move |r: axum::extract::Request, n: axum::middleware::Next| {
                let w = w.clone();
                async move {
                    w.calls.fetch_add(1, Ordering::SeqCst);
                    n.run(r).await
                }
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        StripeBillingClient::with_transport(
            "expiration-key".into(),
            ProviderHttpClient::new(&url, Duration::from_secs(8), 1024 * 1024).unwrap(),
        )
        .unwrap(),
        task,
    )
}
async fn seed(f: &Fixture) -> Uuid {
    let store = f
        .store
        .with_audit_context(AuditContext::actor("service:default", "seed"))
        .unwrap();
    let operation = Uuid::new_v4();
    let request = json!({"amount":1200,"currency":"USD","paymentMethods":["card"],"status":"PENDING","providerFields":{"line_items":[{"price":"price_owned","quantity":1}]}});
    let payload = json!({"request":request,"providerFields":{"line_items":[{"price":"price_owned","quantity":1}],"currency":"USD","payment_method_types":["card"]},"trustedPrices":["price_owned"]});
    let PaymentLinkCreationClaim::Reserved(ticket) = store
        .reserve_payment_link_creation(&f.scope, operation, STRIPE_API_VERSION, &payload)
        .await
        .unwrap()
    else {
        panic!("reserved")
    };
    let id = ticket.public_id();
    let record = json!({"id":id.to_string(),"amount":1200,"currency":"USD","paymentMethods":["card"],"status":"PENDING","providerFields":{"id":format!("plink_{}",id.simple()),"object":"payment_link","livemode":false},"createdAt":"2026-10-15T00:00:00Z","updatedAt":"2026-10-15T00:00:00Z"});
    store
        .complete_payment_link_creation(ticket, record, || Ok(()))
        .await
        .unwrap();
    id
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
    for route in bound_payment_link_expiration_contracts() {
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

fn config() -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"paymentLinkExpiration":{"trustedPrices":["price_owned"],"authority":"internal_service_tenant_v1"}}).to_string()).unwrap()
}
#[tokio::test]
#[ignore = "owned restricted PG, HTTP/MCP and generated SDKs"]
async fn payment_link_expiration_http_mcp_sdk() {
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
async fn selected(f: &Fixture, pool: PgPool) -> BoundBillingV1 {
    let publisher = f
        .store
        .trusted_payment_link_expiration_publisher(pool)
        .await
        .unwrap();
    BoundBillingV1::trusted_with_payment_link_expiration(
        config(),
        None,
        None,
        None,
        Some(publisher),
    )
    .unwrap()
}
// One isolated database lifetime keeps failure cleanup and privilege restoration together.
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {0}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {0}; GRANT EXECUTE ON FUNCTION fl_billing.publish_payment_link_expiration(text,uuid,uuid,text,jsonb,text,text) TO {0}",f.publisher_role))).execute(&f.owner).await.unwrap();
    let pp = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    let w = Wire::default();
    let (provider, provider_task) = server(w.clone()).await;
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    for (table, privilege) in [
        ("records", "SELECT"),
        ("records", "UPDATE"),
        ("provider_bindings", "SELECT"),
        ("provider_bindings", "UPDATE"),
        ("payment_link_creation_intents", "SELECT"),
        ("payment_link_expiration_intents", "SELECT"),
        ("payment_link_expiration_intents", "INSERT"),
        ("payment_link_expiration_dependency_claims", "INSERT"),
        ("payment_link_expiration_audit", "INSERT"),
        ("sensitive_read_audit", "INSERT"),
        ("subscription_publication_identity", "SELECT"),
    ] {
        let selected_profile = selected(f, pp.clone()).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        assert!(
            BillingBackend::stripe((*f.store).clone(), provider.clone())
                .with_bound_billing(selected_profile)
                .await
                .is_err(),
            "{table} {privilege}"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT {privilege} ON fl_billing.{table} TO {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    for (table, privilege) in [
        ("records", "SELECT"),
        ("records", "UPDATE"),
        ("provider_bindings", "SELECT"),
        ("provider_bindings", "UPDATE"),
        ("payment_link_creation_intents", "SELECT"),
        ("payment_link_expiration_intents", "SELECT"),
        ("subscription_publication_identity", "SELECT"),
    ] {
        let selected_profile = selected(f, pp.clone()).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {}",
            f.publisher_role
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        assert!(
            BillingBackend::stripe((*f.store).clone(), provider.clone())
                .with_bound_billing(selected_profile)
                .await
                .is_err(),
            "publisher {table} {privilege}"
        );
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT {privilege} ON fl_billing.{table} TO {}",
            f.publisher_role
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    for backend in [
        BillingBackend::base((*f.store).clone()),
        BillingBackend::stripe((*f.store).clone(), provider.clone()).with_database_backup(true),
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
                .with_bound_billing(selected(f, pp.clone()).await)
                .await
                .is_err()
        );
    }
    assert_eq!(w.calls.load(Ordering::SeqCst), 0);
    let backend = BillingBackend::stripe((*f.store).clone(), provider)
        .with_bound_billing(selected(f, pp.clone()).await)
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
    let task = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    let public = seed(f).await;
    let operation = Uuid::new_v4();
    let path = format!("/bound-payment-links/{public}/expire");
    let input = json!({"operationId":operation});
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
    for bad in [
        json!({}),
        json!({"operationId":"bad"}),
        json!({"operationId":Uuid::nil()}),
        json!({"operationId":operation,"extra":true}),
    ] {
        assert_eq!(
            call(&client, &base, "POST", &path, Some(&bad), "default")
                .await
                .status(),
            400
        );
    }
    assert_eq!(w.calls.load(Ordering::SeqCst), 0);
    let (a, b) = tokio::join!(
        call(&client, &base, "POST", &path, Some(&input), "default"),
        call(&client, &base, "POST", &path, Some(&input), "default")
    );
    assert!([200, 202].contains(&a.status().as_u16()));
    assert!([200, 202].contains(&b.status().as_u16()));
    assert!(a.status() == 200 || b.status() == 200);
    assert_eq!(w.posts.load(Ordering::SeqCst), 1);
    let response = call(&client, &base, "POST", &path, Some(&input), "default").await;
    assert_eq!(response.status(), 200);
    let receipt: Value = response.json().await.unwrap();
    assert_eq!(receipt["state"], "completed");
    assert_eq!(receipt.as_object().unwrap().len(), 3);
    let inspect = format!("/bound-payment-links/{public}/expiration-operations/{operation}");
    let before = w.calls.load(Ordering::SeqCst);
    assert_eq!(
        call(&client, &base, "GET", &inspect, None, "default")
            .await
            .json::<Value>()
            .await
            .unwrap(),
        receipt
    );
    assert_ne!(
        call(&client, &base, "GET", &inspect, None, "other")
            .await
            .status(),
        200
    );
    assert_eq!(w.calls.load(Ordering::SeqCst), before);
    let mcp=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"expireBoundPaymentLink","arguments":{"params":{"publicId":public},"body":input,"headers":{"authorization":sign("POST",&path,Some(&input),"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    let mcp_inspect=client.post(format!("{base}/mcp")).header("accept","application/json, text/event-stream").json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"inspectBoundPaymentLinkExpiration","arguments":{"params":{"publicId":public,"operationId":operation},"headers":{"authorization":sign("GET",&inspect,None,"default")}}}})).send().await.unwrap().json::<Value>().await.unwrap();
    assert_eq!(mcp_inspect["result"]["isError"], false, "{mcp_inspect}");
    sdks(&base, public, operation).await;
    assert_eq!(w.posts.load(Ordering::SeqCst), 1);
    let pending = seed(f).await;
    let op = Uuid::new_v4();
    let path = format!("/bound-payment-links/{pending}/expire");
    let input = json!({"operationId":op});
    *w.invalid.lock().unwrap() = Some(json!({"object":"wrong"}));
    assert_eq!(
        call(&client, &base, "POST", &path, Some(&input), "default")
            .await
            .status(),
        202
    );
    let posts = w.posts.load(Ordering::SeqCst);
    assert_eq!(
        call(&client, &base, "POST", &path, Some(&input), "default")
            .await
            .status(),
        202
    );
    assert_eq!(w.posts.load(Ordering::SeqCst), posts);
    let inspect = format!("/bound-payment-links/{pending}/expiration-operations/{op}");
    let result: Value = call(&client, &base, "GET", &inspect, None, "default")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(result["state"], "pending");
    assert_eq!(result.as_object().unwrap().len(), 3);
    task.abort();
    provider_task.abort();
    pp.close().await;
}
async fn sdks(base: &str, public_id: Uuid, operation_id: Uuid) {
    let mut registry = forklaunch_core::contract::ContractRegistry::new();
    for route in bound_payment_link_expiration_contracts() {
        registry.register(route);
    }
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new(
            "bound-payment-link-expiration",
        ),
    )
    .unwrap();
    let rs = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-payment-link-expiration")
            .with_client_struct("BoundPaymentLinkExpirationClient"),
    )
    .unwrap();
    let root = std::env::temp_dir().join(format!(
        "fl-bound-payment-link-expiration-sdk-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let ts_path = root.join("client.ts");
    let rs_path = root.join("client.rs");
    std::fs::write(&ts_path, ts).unwrap();
    std::fs::write(&rs_path, rs).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_payment_link_expiration_route_sdks.mjs");
    let base = base.to_owned();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts_path)
            .arg(rs_path)
            .arg(base)
            .arg(public_id.to_string())
            .arg(operation_id.to_string())
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
