#![cfg(feature = "bound-checkout")]
#[path = "support/bound_checkout_fixture.rs"]
mod fixture;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use fixture::Fixture;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::client::{STRIPE_API_VERSION, StripeBillingClient};
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundCheckoutConfig, BoundCheckoutV1, bound_checkout_contracts,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use jsonwebtoken::{Algorithm, Validation};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;
#[derive(Clone, Default)]
struct Provider {
    posts: Arc<AtomicUsize>,
    response: Arc<Mutex<Option<Value>>>,
    price: Arc<Mutex<Option<Value>>>,
    items: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer owned-checkout-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
fn price() -> Value {
    json!({"id":"custom-plan","object":"price","livemode":false,"active":true,"type":"recurring","billing_scheme":"per_unit","unit_amount":1200,"currency":"usd","recurring":{"interval":"month","interval_count":1,"usage_type":"licensed"},"product":"custom-product"})
}
fn plan() -> Value {
    json!({"id":"custom-plan","object":"plan","livemode":false,"active":true,"amount":1200,"currency":"usd","interval":"month","interval_count":1,"billing_scheme":"per_unit","usage_type":"licensed","product":{"id":"custom-product","object":"product","livemode":false,"active":true}})
}
fn session(n: usize) -> Value {
    let now = chrono::Utc::now().timestamp();
    json!({"id":format!("cs_owned_{n}"),"object":"checkout.session","livemode":false,"customer":"cus_owned","mode":"subscription","ui_mode":"hosted_page","status":"open","payment_status":"unpaid","payment_method_types":["card"],"success_url":"https://example.test/success","cancel_url":"https://example.test/cancel","currency":"usd","amount_subtotal":2400,"amount_total":2400,"automatic_tax":{"enabled":false},"allow_promotion_codes":false,"subscription":null,"payment_intent":null,"setup_intent":null,"payment_link":null,"invoice":null,"recovered_from":null,"total_details":{"amount_discount":0,"amount_shipping":0,"amount_tax":0},"created":now,"expires_at":now+3600,"url":"https://checkout.stripe.com/c/pay/owned-private"})
}
fn items(id: &str) -> Value {
    json!({"object":"list","has_more":false,"url":format!("/v1/checkout/sessions/{id}/line_items"),"data":[{"id":"li_owned","object":"item","quantity":2,"currency":"usd","amount_subtotal":2400,"amount_total":2400,"amount_discount":0,"amount_tax":0,"price":price()}]})
}
async fn create(State(p): State<Provider>, h: HeaderMap, body: String) -> Json<Value> {
    auth(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-checkout-")
    );
    let fields: std::collections::BTreeMap<String, String> =
        reqwest::Url::parse(&format!("http://fixture/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(a, b)| (a.into_owned(), b.into_owned()))
            .collect();
    assert_eq!(
        fields,
        [
            ("customer", "cus_owned"),
            ("mode", "subscription"),
            ("ui_mode", "hosted_page"),
            ("line_items[0][price]", "custom-plan"),
            ("line_items[0][quantity]", "2"),
            ("payment_method_types[0]", "card"),
            ("success_url", "https://example.test/success"),
            ("cancel_url", "https://example.test/cancel"),
            ("allow_promotion_codes", "false"),
            ("automatic_tax[enabled]", "false"),
            ("adaptive_pricing[enabled]", "false")
        ]
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect()
    );
    let n = p.posts.fetch_add(1, Ordering::SeqCst) + 1;
    if p.pause.load(Ordering::SeqCst) == 1 {
        p.entered.notify_one();
        p.resume.notified().await;
    }
    Json(
        p.response
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| session(n)),
    )
}
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

fn config(f: &Fixture) -> BoundCheckoutConfig {
    BoundCheckoutConfig::from_json(&json!({"deploymentLabel":"owned","accountId":f.scope.account_id(),"livemode":f.scope.livemode(),"plans":[{"localPlanId":f.plan.to_string(),"creationOperationId":f.plan_operation.to_string()}],"successUrl":"https://example.test/success","cancelUrl":"https://example.test/cancel"}).to_string()).unwrap()
}
fn input(f: &Fixture) -> Value {
    json!({"localCustomerId":f.customer.to_string(),"localPlanId":f.plan.to_string(),"quantity":2})
}
fn authentication() -> HmacJwksAuth {
    let mut auth = HmacJwksAuth::new(
        SECRET.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    for route in bound_checkout_contracts() {
        auth = auth
            .with_hmac_keys_for_route(
                &route,
                [
                    ("default".into(), SECRET.to_vec()),
                    ("other".into(), b"other-service-key".to_vec()),
                ],
            )
            .unwrap();
    }
    auth
}
#[tokio::test]
#[ignore = "owned isolated PostgreSQL roles and loopback HTTP/MCP only"]
async fn bound_checkout_http_mcp_operation_lifecycle() {
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
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
    let p = Provider::default();
    let provider = Router::new()
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
            "/v1/customers/cus_owned",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(json!({"id":"cus_owned","object":"customer","livemode":false}))
            }),
        )
        .route(
            "/v1/plans/custom-plan",
            get(|h: HeaderMap| async move {
                auth(&h);
                Json(plan())
            }),
        )
        .route(
            "/v1/prices/custom-plan",
            get(|State(p): State<Provider>, h: HeaderMap| async move {
                auth(&h);
                Json(p.price.lock().unwrap().clone().unwrap_or_else(price))
            }),
        )
        .route("/v1/checkout/sessions", post(create))
        .route(
            "/v1/checkout/sessions/{id}",
            get(|Path(id): Path<String>, h: HeaderMap| async move {
                auth(&h);
                let mut v = session(1);
                v["id"] = json!(id);
                Json(v)
            }),
        )
        .route(
            "/v1/checkout/sessions/{id}/line_items",
            get(
                |State(p): State<Provider>,
                 Path(id): Path<String>,
                 Query(q): Query<std::collections::BTreeMap<String, String>>,
                 h: HeaderMap| async move {
                    auth(&h);
                    assert_eq!(q.get("limit").map(String::as_str), Some("2"));
                    Json(
                        p.items
                            .lock()
                            .unwrap()
                            .clone()
                            .unwrap_or_else(|| items(&id)),
                    )
                },
            ),
        )
        .with_state(p.clone());
    let (provider_url, provider_stop, provider_task) = serve(provider).await;
    let stripe = StripeBillingClient::with_transport(
        "owned-checkout-key".into(),
        ProviderHttpClient::new(&provider_url, Duration::from_secs(7), 1_048_576).unwrap(),
    )
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_checkout(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher_pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    for mode in 0..3 {
        let publisher = f
            .store
            .trusted_checkout_publisher(publisher_pool.clone())
            .await
            .unwrap();
        let backend = match mode {
            0 => BillingBackend::base((*f.store).clone()),
            1 => BillingBackend::stripe((*f.store).clone(), stripe.clone())
                .with_database_backup(true),
            _ => BillingBackend::stripe(
                (*f.store).clone(),
                StripeBillingClient::managed(
                    "https://gateway.example",
                    Uuid::new_v4().to_string(),
                    "fixture-key".into(),
                )
                .unwrap(),
            ),
        };
        assert!(
            backend
                .with_bound_checkout(BoundCheckoutV1::trusted(config(f), publisher))
                .await
                .is_err()
        );
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    let publisher = f
        .store
        .trusted_checkout_publisher(publisher_pool.clone())
        .await
        .unwrap();
    assert!(
        publisher
            .with_audit_context(AuditContext::system("missing-actor"))
            .is_err()
    );
    assert!(
        publisher
            .with_audit_context(AuditContext::actor(" ", "blank-actor"))
            .is_err()
    );
    let backend = BillingBackend::stripe((*f.store).clone(), stripe.clone())
        .with_bound_checkout(BoundCheckoutV1::trusted(config(f), publisher))
        .await
        .unwrap();
    let router =
        BillingHttp::new(backend, authentication(), ServiceInfo::new("billing", "1")).router();
    let legacy = forklaunch_billing_interface::module_contracts::module_registry(true);
    for route in legacy.routes() {
        assert_eq!(
            router
                .registry()
                .routes()
                .iter()
                .find(|r| r.operation_name == route.operation_name)
                .unwrap(),
            route
        );
    }
    for route in bound_checkout_contracts() {
        assert_eq!(
            router
                .registry()
                .routes()
                .iter()
                .find(|r| r.operation_name == route.operation_name)
                .unwrap(),
            &route
        );
    }
    let mut sdk_registry = forklaunch_core::contract::ContractRegistry::new();
    for route in router
        .registry()
        .routes()
        .iter()
        .filter(|r| r.path.starts_with("/bound-checkout/"))
    {
        sdk_registry.register(route.clone());
    }
    let sdk_typescript = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &sdk_registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-checkout"),
    )
    .unwrap();
    let sdk_rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &sdk_registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-checkout")
            .with_client_struct("BoundCheckoutClient"),
    )
    .unwrap();
    let (base, stop, task) = serve(router.into_router_with_default_mcp().unwrap()).await;
    let client = Client::new();
    let operation = Uuid::new_v4();
    let path = format!("/bound-checkout/operations/{operation}");
    assert_eq!(
        client
            .post(format!("{base}{path}"))
            .json(&input(f))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for bad in [
        json!({}),
        json!({"localCustomerId":f.customer.to_string(),"localPlanId":f.plan.to_string(),"quantity":0}),
        json!({"localCustomerId":f.customer.to_string(),"localPlanId":f.plan.to_string(),"quantity":2,"stripeFields":{}}),
    ] {
        let response = call(&client, &base, Method::POST, &path, Some(bad.clone())).await;
        let status = response.status();
        let text = response.text().await.unwrap();
        assert_eq!(status, 400, "invalid request {bad}: {text}");
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    for quantity in [
        Value::Null,
        json!(true),
        json!(-1),
        json!(1.5),
        json!(9_007_199_254_740_992_u64),
        json!("2"),
    ] {
        let mut body = input(f);
        body["quantity"] = quantity;
        assert_eq!(
            call(&client, &base, Method::POST, &path, Some(body))
                .await
                .status(),
            400
        );
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    let created = value(call(&client, &base, Method::POST, &path, Some(input(f))).await).await;
    assert_eq!(created["state"], "created");
    assert_eq!(created["record"]["customerId"], f.customer.to_string());
    let id = created["publicId"].as_str().unwrap();
    assert!(Uuid::parse_str(id).is_ok());
    assert_eq!(created["record"]["stripeFields"]["id"], "cs_owned_1");
    let read = value(
        call(
            &client,
            &base,
            Method::GET,
            &format!("/bound-checkout/sessions/{id}"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(read["id"], id);
    assert_eq!(read["customerId"], f.customer.to_string());
    let duplicate = value(call(&client, &base, Method::POST, &path, Some(input(f))).await).await;
    assert_eq!(duplicate["state"], "completed");
    assert!(duplicate.get("record").is_none());
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let inspection = value(call(&client, &base, Method::GET, &path, None).await).await;
    assert_eq!(inspection, duplicate);
    for path in [&path, &format!("/bound-checkout/sessions/{id}")] {
        let header = generate_hmac_auth_header(
            b"other-service-key",
            "other",
            "GET",
            path,
            SignedBody::Absent,
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            &Uuid::new_v4().to_string(),
        )
        .unwrap();
        let result = client
            .get(format!("{base}{path}"))
            .header("authorization", header)
            .send()
            .await
            .unwrap();
        assert_eq!(result.status(), 409);
    }
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let operation = Uuid::new_v4();
    let path = format!("/bound-checkout/operations/{operation}");
    let body = input(f);
    let raw = body.to_string();
    let mcp=rpc(&client,&base,"tools/call",json!({"name":"createBoundCheckoutSession","arguments":{"params":{"operationId":operation.to_string()},"body":body,"headers":{"authorization":sign("POST",&path,Some(&raw))}}})).await;
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    assert_eq!(p.posts.load(Ordering::SeqCst), 2);
    p.pause.store(1, Ordering::SeqCst);
    let concurrent_operation = Uuid::new_v4();
    let concurrent_path = format!("/bound-checkout/operations/{concurrent_operation}");
    let first = call(
        &client,
        &base,
        Method::POST,
        &concurrent_path,
        Some(input(f)),
    );
    tokio::pin!(first);
    tokio::select! {()=p.entered.notified()=>{},result=&mut first=>panic!("must pause {}",result.status())}
    assert_eq!(
        call(
            &client,
            &base,
            Method::POST,
            &concurrent_path,
            Some(input(f))
        )
        .await
        .status(),
        202
    );
    let mut changed = input(f);
    changed["quantity"] = json!(3);
    assert_eq!(
        call(
            &client,
            &base,
            Method::POST,
            &concurrent_path,
            Some(changed)
        )
        .await
        .status(),
        409
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 3);
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    assert_eq!(first.await.status(), 200);
    route_sdks(sdk_typescript, sdk_rust, &base, f).await;
    assert_eq!(p.posts.load(Ordering::SeqCst), 5);
    // Provider effect occurred, malformed result leaves a durable pending operation.
    let operation = Uuid::new_v4();
    let path = format!("/bound-checkout/operations/{operation}");
    *p.response.lock().unwrap() = Some(json!({"id":"cs_malformed","object":"wrong"}));
    let result = call(&client, &base, Method::POST, &path, Some(input(f))).await;
    assert_eq!(result.status(), 202);
    let pending: Value = result.json().await.unwrap();
    assert_eq!(pending["state"], "pending");
    let posts = p.posts.load(Ordering::SeqCst);
    let result = call(&client, &base, Method::POST, &path, Some(input(f))).await;
    assert_eq!(result.status(), 202);
    assert_eq!(p.posts.load(Ordering::SeqCst), posts);
    let pending = value(call(&client, &base, Method::GET, &path, None).await).await;
    assert_eq!(pending["state"], "pending");
    assert!(pending.get("record").is_none());
    assert_eq!(f.count("checkout_dependency_claims").await, 4);
    f.denied(
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "checkout-a",
    )
    .await;
    stop.send(()).unwrap();
    task.await.unwrap();
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
    publisher_pool.close().await;
}

async fn route_sdks(typescript: String, rust: String, base: &str, f: &Fixture) {
    let root = std::env::temp_dir().join(format!(
        "fl-bound-checkout-route-clients-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let ts = root.join("client.ts");
    let rs = root.join("client.rs");
    std::fs::write(&ts, typescript).unwrap();
    std::fs::write(&rs, rust).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_checkout_route_sdks.mjs");
    let base = base.to_owned();
    let customer = f.customer.to_string();
    let plan = f.plan.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts)
            .arg(rs)
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
        "SDK proof: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
