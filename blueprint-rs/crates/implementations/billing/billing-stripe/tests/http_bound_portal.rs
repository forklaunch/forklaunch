#![cfg(feature = "bound-checkout")]
#[path = "support/bound_portal_fixture.rs"]
mod fixture;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use fixture::Fixture;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_stripe::client::{STRIPE_API_VERSION, StripeBillingClient};
use forklaunch_billing_stripe::http::{
    BillingBackend, BillingHttp, BoundBillingConfig, BoundBillingV1, bound_portal_contracts,
    bound_portal_expiration_contracts,
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
    configuration: Arc<Mutex<Option<Value>>>,
    pause: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}
fn auth(h: &HeaderMap) {
    assert_eq!(h["authorization"], "Bearer owned-portal-key");
    assert_eq!(h["stripe-version"], STRIPE_API_VERSION);
    assert!(!h.contains_key("stripe-account"));
}
fn session(n: usize) -> Value {
    json!({"id":format!("bps_owned_{n}"),"object":"billing_portal.session","livemode":false,"customer":"cus_owned","configuration":"bpc_owned","return_url":"https://example.test/return","on_behalf_of":null,"flow":null,"created":chrono::Utc::now().timestamp(),"url":"https://billing.stripe.com/p/session/private"})
}
async fn create(State(p): State<Provider>, h: HeaderMap, body: String) -> Json<Value> {
    auth(&h);
    assert!(
        h["idempotency-key"]
            .to_str()
            .unwrap()
            .starts_with("fl-portal-")
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
            ("configuration", "bpc_owned"),
            ("return_url", "https://example.test/return")
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

fn config(f: &Fixture) -> BoundBillingConfig {
    BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":f.scope.account_id(),"livemode":f.scope.livemode(),"portal":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"},"portalLocalExpiration":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"}}).to_string()).unwrap()
}
fn input(f: &Fixture) -> Value {
    json!({"localCustomerId":f.customer.to_string()})
}
fn authentication() -> HmacJwksAuth {
    let mut auth = HmacJwksAuth::new(
        SECRET.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    for route in bound_portal_contracts()
        .into_iter()
        .chain(bound_portal_expiration_contracts())
    {
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
async fn bound_portal_http_mcp_operation_lifecycle() {
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
        .route("/v1/billing_portal/configurations/bpc_owned", get(|State(p):State<Provider>,h:HeaderMap|async move { auth(&h);Json(p.configuration.lock().unwrap().clone().unwrap_or_else(||json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":false,"active":true}))) }))
        .route("/v1/billing_portal/sessions", post(create))
        .with_state(p.clone());
    let (provider_url, provider_stop, provider_task) = serve(provider).await;
    let stripe = StripeBillingClient::with_transport(
        "owned-portal-key".into(),
        ProviderHttpClient::new(&provider_url, Duration::from_secs(7), 1_048_576).unwrap(),
    )
    .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT USAGE ON SCHEMA fl_billing TO {}; GRANT ALL ON ALL TABLES IN SCHEMA fl_billing TO {}; GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role,f.publisher_role,f.publisher_role))).execute(&f.owner).await.unwrap();
    let publisher_pool = PgPoolOptions::new()
        .max_connections(3)
        .connect_with(
            (*f.pool.connect_options())
                .clone()
                .username(&f.publisher_role),
        )
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) FROM {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    assert!(
        f.store
            .trusted_portal_publisher(publisher_pool.clone())
            .await
            .is_err()
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_portal(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    // Combined selection validates each independent publication grant; no provider I/O.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT EXECUTE ON FUNCTION fl_billing.publish_bound_checkout(text,uuid,uuid,text,jsonb,jsonb,text,text,text) TO {}",f.publisher_role))).execute(&f.owner).await.unwrap();
    let combined=BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"portal":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"},"checkout":{"plans":[{"localPlanId":Uuid::new_v4(),"creationOperationId":Uuid::new_v4()}],"successUrl":"https://example.test/success","cancelUrl":"https://example.test/cancel"}}).to_string()).unwrap();
    let selected = combined.contracts();
    let combined = BoundBillingV1::trusted(
        combined,
        Some(
            f.store
                .trusted_checkout_publisher(publisher_pool.clone())
                .await
                .unwrap(),
        ),
        Some(
            f.store
                .trusted_portal_publisher(publisher_pool.clone())
                .await
                .unwrap(),
        ),
    )
    .unwrap();
    let combined = BillingBackend::stripe((*f.store).clone(), stripe.clone())
        .with_bound_billing(combined)
        .await
        .unwrap();
    let combined =
        BillingHttp::new(combined, authentication(), ServiceInfo::new("billing", "1")).router();
    assert_eq!(selected.len(), 6);
    for route in selected {
        assert!(combined.registry().routes().contains(&route));
    }
    assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    for mode in 0..3 {
        let publisher = f
            .store
            .trusted_portal_publisher(publisher_pool.clone())
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
                .with_bound_billing(
                    BoundBillingV1::trusted(config(f), None, Some(publisher)).unwrap()
                )
                .await
                .is_err()
        );
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    expiration_startup(f, &stripe, &publisher_pool).await;
    let publisher = f
        .store
        .trusted_portal_publisher(publisher_pool.clone())
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
        .with_bound_billing(BoundBillingV1::trusted(config(f), None, Some(publisher)).unwrap())
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
    for route in bound_portal_contracts()
        .into_iter()
        .chain(bound_portal_expiration_contracts())
    {
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
        .filter(|r| r.path.starts_with("/bound-portal/"))
    {
        sdk_registry.register(route.clone());
    }
    let sdk_typescript = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            &sdk_registry,
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("bound-portal"),
    )
    .unwrap();
    let sdk_rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::Rust,
            &sdk_registry,
        ),
        &forklaunch_core::rust_sdk::RustSdkOptions::new("bound-portal")
            .with_client_struct("BoundPortalClient"),
    )
    .unwrap();
    let (base, stop, task) = serve(router.into_router_with_default_mcp().unwrap()).await;
    let client = Client::new();
    let operation = Uuid::new_v4();
    let path = format!("/bound-portal/operations/{operation}");
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
        json!({"localCustomerId":null}),
        json!({"localCustomerId":17}),
        json!({"localCustomerId":Uuid::nil().to_string()}),
        json!({"localCustomerId":f.customer.to_string(),"stripeFields":{}}),
    ] {
        let response = call(&client, &base, Method::POST, &path, Some(bad.clone())).await;
        let status = response.status();
        let text = response.text().await.unwrap();
        assert_eq!(status, 400, "invalid request {bad}: {text}");
        assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    }
    *p.configuration.lock().unwrap() = Some(
        json!({"id":"bpc_owned","object":"billing_portal.configuration","livemode":false,"active":false}),
    );
    assert_eq!(
        call(&client, &base, Method::POST, &path, Some(input(f)))
            .await
            .status(),
        400
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    *p.configuration.lock().unwrap() = None;
    assert!(
        !call(
            &client,
            &base,
            Method::POST,
            &path,
            Some(json!({"localCustomerId":Uuid::new_v4()}))
        )
        .await
        .status()
        .is_success()
    );
    assert_eq!(p.posts.load(Ordering::SeqCst), 0);
    let created = value(call(&client, &base, Method::POST, &path, Some(input(f))).await).await;
    assert_eq!(created["state"], "created");
    assert_eq!(created["record"]["customerId"], f.customer.to_string());
    let id = created["publicId"].as_str().unwrap();
    assert!(Uuid::parse_str(id).is_ok());
    assert_eq!(created["record"]["stripeFields"]["id"], "bps_owned_1");
    let stored = value(
        call(
            &client,
            &base,
            Method::GET,
            &format!("/bound-portal/sessions/{id}"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(stored["state"], "stored");
    assert_eq!(stored["record"], created["record"]);
    assert!(
        chrono::DateTime::parse_from_rfc3339(stored["observedAt"].as_str().unwrap()).unwrap()
            >= chrono::DateTime::parse_from_rfc3339(
                created["record"]["createdAt"].as_str().unwrap()
            )
            .unwrap()
    );
    let get_path = format!("/bound-portal/sessions/{id}");
    let mcp=rpc(&client,&base,"tools/call",json!({"name":"getStoredBoundPortalSession","arguments":{"params":{"publicId":id},"headers":{"authorization":sign("GET",&get_path,None)}}})).await;
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let duplicate = value(call(&client, &base, Method::POST, &path, Some(input(f))).await).await;
    assert_eq!(duplicate["state"], "completed");
    assert!(duplicate.get("record").is_none());
    assert_eq!(p.posts.load(Ordering::SeqCst), 1);
    let inspection = value(call(&client, &base, Method::GET, &path, None).await).await;
    assert_eq!(inspection, duplicate);
    for path in [&path, &get_path] {
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
    let path = format!("/bound-portal/operations/{operation}");
    let body = input(f);
    let raw = body.to_string();
    let mcp=rpc(&client,&base,"tools/call",json!({"name":"createBoundPortalSession","arguments":{"params":{"operationId":operation.to_string()},"body":body,"headers":{"authorization":sign("POST",&path,Some(&raw))}}})).await;
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    assert_eq!(p.posts.load(Ordering::SeqCst), 2);
    p.pause.store(1, Ordering::SeqCst);
    let concurrent_operation = Uuid::new_v4();
    let concurrent_path = format!("/bound-portal/operations/{concurrent_operation}");
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
    assert_eq!(p.posts.load(Ordering::SeqCst), 3);
    p.pause.store(0, Ordering::SeqCst);
    p.resume.notify_one();
    assert_eq!(first.await.status(), 200);
    expiration_routes(&client, &base, id).await;
    route_sdks(sdk_typescript, sdk_rust, &base, f).await;
    assert_eq!(p.posts.load(Ordering::SeqCst), 5);
    expired_read(f, &publisher_pool, &client, &base).await;
    // Provider effect occurred, malformed result leaves a durable pending operation.
    let operation = Uuid::new_v4();
    let path = format!("/bound-portal/operations/{operation}");
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
    assert_eq!(f.count("portal_dependency_claims").await, 2);
    f.denied(
        "DELETE FROM fl_billing.stripe_customer_bindings",
        "portal-a",
    )
    .await;
    stop.send(()).unwrap();
    task.await.unwrap();
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
    publisher_pool.close().await;
}

async fn route_sdks(typescript: String, rust: String, base: &str, f: &Fixture) {
    let root =
        std::env::temp_dir().join(format!("fl-bound-portal-route-clients-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let ts = root.join("client.ts");
    let rs = root.join("client.rs");
    std::fs::write(&ts, typescript).unwrap();
    std::fs::write(&rs, rust).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_bound_portal_route_sdks.mjs");
    let base = base.to_owned();
    let customer = f.customer.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("node")
            .arg(script)
            .arg(ts)
            .arg(rs)
            .arg(base)
            .arg(customer)
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

// Explicit trusted publication fixture proves route410 without waiting five minutes.
// It does not represent an actual provider creation or a new financial operation.
async fn expired_read(f: &Fixture, pool: &sqlx::PgPool, client: &Client, base: &str) {
    let store = f
        .store
        .with_audit_context(AuditContext::actor(
            "service:default",
            "expired-read-fixture",
        ))
        .unwrap();
    let publisher = store.trusted_portal_publisher(pool.clone()).await.unwrap();
    let operation = Uuid::new_v4();
    let snapshot = store
        .snapshot_portal_dependencies(&f.scope, f.customer, STRIPE_API_VERSION)
        .await
        .unwrap();
    let payload = json!({"profile":"direct_portal_v1","customer":f.customer.to_string(),"fields":{"customer":"cus_owned","configuration":"bpc_owned","return_url":"https://example.test/return"}});
    let forklaunch_billing_base::services::postgres::PortalCreationClaim::Reserved(ticket) = store
        .reserve_portal_creation(snapshot, operation, &payload, || Ok(()))
        .await
        .unwrap()
    else {
        panic!("fresh fixture")
    };
    let id = ticket.public_id();
    let created = chrono::Utc::now() - chrono::Duration::minutes(6);
    let provider = json!({"id":format!("bps_{}",id.simple()),"object":"billing_portal.session","livemode":false,"customer":"cus_owned","configuration":"bpc_owned","return_url":"https://example.test/return","url":"https://billing.stripe.com/p/session/private"});
    let record = json!({"id":id.to_string(),"customerId":f.customer.to_string(),"providerFields":provider,"uri":provider["url"],"createdAt":created,"updatedAt":created,"expiresAt":created+chrono::Duration::minutes(5)});
    publisher.complete(ticket, record, || Ok(())).await.unwrap();
    let response = call(
        client,
        base,
        Method::GET,
        &format!("/bound-portal/sessions/{id}"),
        None,
    )
    .await;
    assert_eq!(response.status(), 410);
    assert!(!response.text().await.unwrap().contains("https://"));
    let path = format!("/bound-portal/sessions/{id}");
    let mcp=rpc(client,base,"tools/call",json!({"name":"getStoredBoundPortalSession","arguments":{"params":{"publicId":id.to_string()},"headers":{"authorization":sign("GET",&path,None)}}})).await;
    assert_eq!(mcp["result"]["isError"], true, "{mcp}");
    assert!(!mcp.to_string().contains("https://"));
    let inspection = value(
        call(
            client,
            base,
            Method::GET,
            &format!("/bound-portal/operations/{operation}"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(inspection["state"], "completed");
    assert!(inspection.get("record").is_none());
}

async fn expiration_routes(client: &Client, base: &str, id: &str) {
    let status = format!("/bound-portal/sessions/{id}/expiration");
    let expire = format!("/bound-portal/sessions/{id}/expire");
    assert_eq!(
        value(call(client, base, Method::GET, &status, None).await).await["state"],
        "notRecorded"
    );
    for _ in 0..2 {
        assert_eq!(
            value(call(client, base, Method::POST, &expire, None).await).await["state"],
            "localExpired"
        );
    }
    assert_eq!(
        call(
            client,
            base,
            Method::GET,
            &format!("/bound-portal/sessions/{id}"),
            None
        )
        .await
        .status(),
        410
    );
    let result=rpc(client,base,"tools/call",json!({"name":"expireStoredBoundPortalSession","arguments":{"params":{"publicId":id},"headers":{"authorization":sign("POST",&expire,None)}}})).await;
    assert_eq!(result["result"]["isError"], false, "{result}");
    let result=rpc(client,base,"tools/call",json!({"name":"inspectStoredBoundPortalExpiration","arguments":{"params":{"publicId":id},"headers":{"authorization":sign("GET",&status,None)}}})).await;
    assert_eq!(result["result"]["isError"], false, "{result}");
}

async fn expiration_startup(
    f: &Fixture,
    stripe: &StripeBillingClient,
    publisher_pool: &sqlx::PgPool,
) {
    let configured = || {
        BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"portalLocalExpiration":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"}}).to_string()).unwrap()
    };
    for backend in [
        BillingBackend::base((*f.store).clone()),
        BillingBackend::stripe((*f.store).clone(), stripe.clone()).with_database_backup(true),
        BillingBackend::stripe(
            (*f.store).clone(),
            StripeBillingClient::managed(
                "https://gateway.example",
                Uuid::new_v4().to_string(),
                "fixture-key".into(),
            )
            .unwrap(),
        ),
    ] {
        assert!(
            backend
                .with_bound_billing(BoundBillingV1::trusted(configured(), None, None).unwrap())
                .await
                .is_err()
        );
    }
    let role = f.pool.connect_options().get_username().to_owned();
    for (table, privilege) in [
        ("portal_local_expirations", "SELECT"),
        ("portal_local_expirations", "INSERT"),
        ("records", "UPDATE"),
        ("sensitive_read_audit", "INSERT"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE {privilege} ON fl_billing.{table} FROM {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        assert!(
            BillingBackend::stripe((*f.store).clone(), stripe.clone())
                .with_bound_billing(BoundBillingV1::trusted(configured(), None, None).unwrap())
                .await
                .is_err()
        );
        if table == "portal_local_expirations" && privilege == "SELECT" {
            let old=BoundBillingConfig::from_json(&json!({"deploymentLabel":"owned","accountId":"acct_owned","livemode":false,"portal":{"configurationId":"bpc_owned","returnUrl":"https://example.test/return"}}).to_string()).unwrap();
            let publisher = f
                .store
                .trusted_portal_publisher(publisher_pool.clone())
                .await
                .unwrap();
            assert!(
                BillingBackend::stripe((*f.store).clone(), stripe.clone())
                    .with_bound_billing(
                        BoundBillingV1::trusted(old, None, Some(publisher)).unwrap()
                    )
                    .await
                    .is_err()
            );
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT {privilege} ON fl_billing.{table} TO {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
    }
    let backend = BillingBackend::stripe((*f.store).clone(), stripe.clone())
        .with_bound_billing(BoundBillingV1::trusted(configured(), None, None).unwrap())
        .await
        .unwrap();
    let router =
        BillingHttp::new(backend, authentication(), ServiceInfo::new("billing", "1")).router();
    assert_eq!(
        router.registry().routes().len(),
        forklaunch_billing_interface::module_contracts::module_registry(true)
            .routes()
            .len()
            + 2
    );
    assert!(
        !router
            .registry()
            .routes()
            .iter()
            .any(|r| r.operation_name == "createBoundPortalSession"
                || r.operation_name == "getStoredBoundPortalSession")
    );
}
