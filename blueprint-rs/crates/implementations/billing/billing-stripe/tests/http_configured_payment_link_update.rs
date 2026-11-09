#![cfg(feature = "http")]
#[path = "support/bound_portal_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use axum::{
    Json, Router,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use fixture::Fixture;
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::{BillingKind, BillingStore, Scope};
use forklaunch_billing_stripe::http::{BillingBackend, BillingHttp};
use forklaunch_core::auth::{
    jwt::{RemoteJwks, RemoteJwksConfig},
    request_hmac::{SignedBody, generate_hmac_auth_header},
};

use forklaunch_billing_stripe::client::StripeBillingClient;
use forklaunch_core::ServiceInfo;
use forklaunch_core::http::provider_http::ProviderHttpClient;
use jsonwebtoken::{Algorithm, Validation};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use uuid::Uuid;
const KEY: &[u8] = b"billing-owner-fixture";
fn sign(method: &str, path: &str, body: Option<&Value>, actor: &str) -> String {
    let encoded = body.map(Value::to_string);
    generate_hmac_auth_header(
        KEY,
        actor,
        method,
        // The full wire path, as the SDK signers sign it (no query string).
        path.split('?').next().unwrap(),
        encoded
            .as_deref()
            .map_or(SignedBody::Absent, SignedBody::Text),
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        &Uuid::new_v4().to_string(),
    )
    .unwrap()
}

#[derive(Clone, Default)]
struct Provider {
    calls: Arc<Mutex<Vec<String>>>,
    response: Arc<Mutex<Value>>,
    status: Arc<Mutex<Option<u16>>>,
}
async fn provider(
    State(state): State<Provider>,
    method: Method,
    uri: Uri,
    body: axum::body::Bytes,
) -> Response {
    assert!(
        !String::from_utf8_lossy(&body).contains("amount"),
        "requested amount must not be forwarded"
    );
    state
        .calls
        .lock()
        .await
        .push(format!("{method} {}", uri.path()));
    let status = *state.status.lock().await;
    if status == Some(599) {
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    (
        StatusCode::from_u16(status.unwrap_or(200)).unwrap(),
        Json(state.response.lock().await.clone()),
    )
        .into_response()
}
#[tokio::test]
#[ignore = "owned restricted PostgreSQL configured PaymentLink update HTTP"]
async fn configured_payment_link_update_backend() {
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
async fn update(
    http: &reqwest::Client,
    url: &str,
    id: &str,
    body: &Value,
    correlation: Option<&str>,
) -> reqwest::Response {
    let path = format!("/payment-link/{id}");
    let mut request = http
        .put(format!("{url}{path}"))
        .header("authorization", sign("PUT", &path, Some(body), "default"))
        .json(body);
    if let Some(value) = correlation {
        request = request.header("x-correlation-id", value);
    }
    request.send().await.unwrap()
}
#[allow(clippy::too_many_lines)]
async fn matrix(f: &Fixture) {
    let state = Provider::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().fallback(provider).with_state(state.clone());
    let provider_task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeBillingClient::with_transport(
        "sk_fixture".into(),
        ProviderHttpClient::new(&provider_url, Duration::from_millis(100), 4096).unwrap(),
    )
    .unwrap();
    let backend = BillingBackend::stripe((*f.store).clone(), client)
        .with_configured_hmac(true)
        .with_configured_payment_link_updates(true);
    let auth = HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    let router = BillingHttp::new(backend, auth, ServiceInfo::new("billing", "1")).router();
    let sdk = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &forklaunch_core::codegen::SdkGenerationPlan::from_registry(
            forklaunch_core::codegen::SdkTarget::TypeScript,
            router.registry(),
        ),
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new("billing"),
    )
    .unwrap();
    let selected = router
        .registry()
        .routes()
        .iter()
        .find(|route| route.operation_name == "updatePaymentLink")
        .unwrap();
    let metadata: Value = serde_json::from_str(
        selected
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
    assert_eq!(
        metadata["properties"]["amount"]["x-forklaunch-coercion"],
        "typescript-number"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let http = reqwest::Client::new();
    let id = Uuid::new_v4().to_string();
    let cached = json!({"id":id,"amount":12,"currency":"USD","status":"PENDING","paymentMethods":["card"],"providerFields":{"id":"plink_retained","object":"payment_link","livemode":false}});
    *state.response.lock().await = json!({"id":"plink_retained","object":"payment_link","line_items":{"data":[{"amount_total":34}]}});
    for correlation in [None, Some("preflight"), Some("")] {
        let response = update(&http, &url, &id, &json!({}), correlation).await;
        assert_eq!(response.status(), 500);
        let observed = response.headers()["x-correlation-id"]
            .to_str()
            .unwrap()
            .to_owned();
        if let Some(value) = correlation.filter(|value| !value.is_empty()) {
            assert_eq!(observed, value);
        } else {
            assert!(Uuid::parse_str(&observed).is_ok());
        }
        assert_eq!(
            response.text().await.unwrap(),
            format!("Internal server error:\n\nCorrelation id: {observed}")
        );
        assert!(state.calls.lock().await.is_empty());
    }
    f.store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            cached.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    // Invalid conversions remain admission failures before any service I/O.
    for patch in [
        json!({"status":"INVALID"}),
        json!({"currency":"INVALID"}),
        json!({"paymentMethods":["invalid"]}),
        json!({"amount":"bad"}),
        json!({"amount":{}}),
        json!({"amount":[1,2]}),
    ] {
        assert_eq!(update(&http, &url, &id, &patch, None).await.status(), 400);
        assert!(state.calls.lock().await.is_empty());
    }
    for (field, value) in [
        ("id", json!("other")),
        ("createdAt", json!("bad-date")),
        ("status", json!("INVALID")),
    ] {
        let mut tx = f.owner.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        let original:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id='portal-a' AND scope='cache' AND kind='payment_link' AND id=$1").bind(&id).fetch_one(&mut *tx).await.unwrap();
        sqlx::query("UPDATE fl_billing.records SET data=jsonb_set(data,ARRAY[$2::text],$3) WHERE tenant_id='portal-a' AND scope='cache' AND kind='payment_link' AND id=$1").bind(&id).bind(field).bind(sqlx::types::Json(value)).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let response = update(&http, &url, &id, &json!({}), Some("bad-cache")).await;
        assert_eq!(response.status(), 500);
        assert_eq!(
            response.text().await.unwrap(),
            "Internal server error:\n\nCorrelation id: bad-cache"
        );
        assert!(state.calls.lock().await.is_empty());
        let mut tx = f.owner.begin().await.unwrap();
        sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE fl_billing.records SET data=$2 WHERE tenant_id='portal-a' AND scope='cache' AND kind='payment_link' AND id=$1").bind(&id).bind(original).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    for status in [404, 429, 500, 599] {
        *state.status.lock().await = Some(status);
        let response = update(&http, &url, &id, &json!({}), Some("provider")).await;
        let expected = if status == 599 { 500 } else { status };
        assert_eq!(response.status().as_u16(), expected);
        assert_eq!(
            response.text().await.unwrap(),
            format!(
                "{}:\n\nCorrelation id: provider",
                if expected >= 500 {
                    "Internal server error"
                } else {
                    "Request could not be processed"
                }
            )
        );
        assert_eq!(
            f.store
                .get(Scope::Cache, BillingKind::PaymentLink, &id)
                .await
                .unwrap(),
            Some(cached.clone())
        );
    }
    *state.status.lock().await = None;
    state.calls.lock().await.clear();
    let response = update(&http, &url, &id, &json!({"status":"COMPLETED"}), None).await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "COMPLETED");
    assert_eq!(body["amount"], 34.0);
    assert_eq!(
        *state.calls.lock().await,
        vec!["POST /v1/payment_links/plink_retained".to_string()]
    );
    // 5af4a0f70: Stripe is called with the stored Stripe id; the public id stays local.
    assert_eq!(body["stripeFields"]["id"], "plink_retained");
    assert_eq!(body["id"], id);
    // Actual shared TS router runs parse before auth. Invalid conversions win
    // over invalid signatures; successful coercion must still authenticate raw bytes.
    // Selected configured HMAC matches actual TS 403 for invalid signatures.
    for (amount, expected) in [
        (json!("bad"), 400),
        (json!({}), 400),
        (json!(null), 403),
        (json!("12"), 403),
    ] {
        state.calls.lock().await.clear();
        let body = json!({"amount":amount});
        let path = format!("/payment-link/{id}");
        let invalid_signature = sign("PUT", &path, Some(&json!({"different":"body"})), "default");
        let response = http
            .put(format!("{url}{path}"))
            .header("authorization", invalid_signature)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected, "amount={amount}");
        assert!(state.calls.lock().await.is_empty());
    }
    for amount in [
        json!(null),
        json!("12"),
        json!(false),
        json!([]),
        json!(true),
        json!(""),
        json!("  "),
        json!("0x10"),
        json!([12]),
        json!([[12]]),
        json!("Infinity"),
        json!("-Infinity"),
        json!("1e999"),
    ] {
        state.calls.lock().await.clear();
        let response = update(
            &http,
            &url,
            &id,
            &json!({"amount":amount}),
            Some("coercion"),
        )
        .await;
        assert_eq!(response.status(), 200, "amount={amount}");
        assert_eq!(response.json::<Value>().await.unwrap()["amount"], 34.0);
        assert_eq!(state.calls.lock().await.len(), 1);
    }
    for amount in [
        json!("bad"),
        json!({}),
        json!([1, 2]),
        json!({"toString":"bad"}),
    ] {
        state.calls.lock().await.clear();
        let response = update(
            &http,
            &url,
            &id,
            &json!({"amount":amount}),
            Some("parse-control"),
        )
        .await;
        assert_eq!(response.status(), 400);
        let text = response.text().await.unwrap();
        assert!(text.contains("Path: body > amount"), "{text}");
        assert!(text.ends_with("Correlation id: parse-control"));
        assert!(state.calls.lock().await.is_empty());
    }
    for amount in [
        json!(null),
        json!("12"),
        json!(false),
        json!([]),
        json!("Infinity"),
        json!("bad"),
    ] {
        state.calls.lock().await.clear();
        let path = format!("/payment-link/{id}");
        let body = json!({"amount":amount});
        let response=http.post(format!("{url}/mcp")).header("accept","application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"updatePaymentLink","arguments":{"params":{"id":id},"body":body,"headers":{"authorization":sign("PUT",&path,Some(&body),"default")}}}})).send().await.unwrap();
        let result: Value = response.json().await.unwrap();
        let expected = if amount == "bad" { 400 } else { 200 };
        assert_eq!(result["result"]["isError"], expected != 200, "{result}");
        if expected == 200 {
            assert_eq!(
                result["result"]["structuredContent"]["amount"], 34.0,
                "{result}"
            );
        }
        assert_eq!(state.calls.lock().await.len(), usize::from(expected == 200));
    }
    let file = std::env::temp_dir().join(format!("fl-link-coercion-sdk-{}.ts", Uuid::new_v4()));
    std::fs::write(&file, sdk).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../framework-rs/parity/rust_billing_payment_link_coercion_sdk.mjs");
    let sdk_url = url.clone();
    let sdk_id = id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let result = std::process::Command::new("node")
            .arg(script)
            .arg(&file)
            .arg(sdk_url)
            .arg(sdk_id)
            .output()
            .unwrap();
        std::fs::remove_file(file).unwrap();
        result
    })
    .await
    .unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
    hmac_matrix(&http, &url, &id, &state).await;
    server.abort();
    provider_task.abort();
}

#[allow(clippy::too_many_lines)] // One finite header/correlation matrix shares the live provider counters.
async fn hmac_matrix(http: &reqwest::Client, url: &str, id: &str, state: &Provider) {
    let path = format!("/payment-link/{id}");
    let body = json!({"amount":"12"});
    let valid = sign("PUT", &path, Some(&body), "default");
    let mut cases = vec![
        (None, 401, "Authorization token required."),
        (
            Some(String::new()),
            401,
            "Invalid Authorization token format.",
        ),
        (
            Some("HMAC".into()),
            401,
            "Invalid Authorization token format.",
        ),
        (
            Some("HMAC keyId=default".into()),
            403,
            "Invalid Authorization token.",
        ),
        (
            Some(valid.replace("HMAC", "OTHER")),
            403,
            "Invalid Authorization token.",
        ),
        (
            Some(valid.replace("keyId=", "key=")),
            401,
            "Invalid Authorization token format.",
        ),
        (
            Some(valid.replace("keyId=default", "keyId=unknown")),
            500,
            "",
        ),
        (
            Some(valid.replace("signature=", "signature=bad")),
            403,
            "Invalid Authorization signature.",
        ),
        (Some(valid.clone()), 200, ""),
        (Some(valid), 200, ""),
    ];
    for stamp in ["1900-01-01T00:00:00.000Z", "2100-01-01T00:00:00.000Z"] {
        let payload = format!("PUT\n/payment-link/{id}\n{body}\n{stamp}\nrepeat");
        let signature = forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes());
        cases.push((
            Some(format!(
                "HMAC keyId=default ts={stamp} nonce=repeat signature={signature} ignored=extra"
            )),
            200,
            "",
        ));
    }
    if let Ok(reference) = std::env::var("FORKLAUNCH_HMAC_DATE_CASES") {
        let reference: Vec<Value> = serde_json::from_str(&reference).unwrap();
        for case in reference {
            let stamp = case["timestamp"].as_str().unwrap();
            let canonical = case["canonical"]
                .as_str()
                .unwrap_or("2000-01-01T00:00:00.000Z");
            let payload = format!("PUT\n/payment-link/{id}\n{body}\n{canonical}\nrepeat");
            let signature = forklaunch_core::auth::hmac_sign(KEY, payload.as_bytes());
            cases.push((
                Some(format!(
                    "HMAC keyId=default ts={stamp} nonce=repeat signature={signature}"
                )),
                u16::try_from(case["status"].as_u64().unwrap()).unwrap(),
                "",
            ));
        }
    }
    for (authorization, status, message) in cases {
        for correlation in [None, Some("hmac-correlation"), Some("")] {
            state.calls.lock().await.clear();
            let mut request = http.put(format!("{url}{path}")).json(&body);
            if let Some(value) = &authorization {
                request = request.header("authorization", value);
            }
            if let Some(value) = correlation {
                request = request.header("x-correlation-id", value);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status().as_u16(), status, "{authorization:?}");
            let observed = response.headers()["x-correlation-id"]
                .to_str()
                .unwrap()
                .to_owned();
            let text = response.text().await.unwrap();
            if status == 500 {
                assert_eq!(
                    text,
                    format!("Internal server error:\n\nCorrelation id: {observed}")
                );
            } else if status != 200 {
                assert_eq!(text, message);
            } else {
                let value: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(value["amount"], 34.0);
            }
            assert_eq!(state.calls.lock().await.len(), usize::from(status == 200));
        }
    }
    state.calls.lock().await.clear();
    for (authorization, status) in [(None, 401), (Some("Bearer invalid"), 403)] {
        let mut request = http.get(format!("{url}/billing-portal/{id}"));
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
        let _: Value = response.json().await.unwrap();
        assert!(state.calls.lock().await.is_empty());
    }
    state.calls.lock().await.clear();
    let trailing = http
        .put(format!("{url}{path}/"))
        .header(
            "authorization",
            sign("PUT", &format!("{path}/"), Some(&body), "default"),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(
        trailing.status(),
        200,
        "configured native alias matches Express and preserves signed slash"
    );
    assert_eq!(state.calls.lock().await.len(), 1);
    state.calls.lock().await.clear();
    let wrong = http
        .put(format!("{url}{path}/"))
        .header("authorization", sign("PUT", &path, Some(&body), "default"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 403);
    assert!(state.calls.lock().await.is_empty());
    let invalid_body = http
        .put(format!("{url}{path}/"))
        .header("authorization", "bad")
        .json(&json!({"amount":"bad"}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_body.status(), 400);
    assert!(state.calls.lock().await.is_empty());
    for path in [format!("{path}//"), format!("/billing-portal/{id}/")] {
        let rejected = http
            .put(format!("{url}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), 404);
        assert!(state.calls.lock().await.is_empty());
    }
    let uppercase = format!("/PAYMENT-LINK/{id}");
    let response = http
        .put(format!("{url}{uppercase}"))
        .header(
            "authorization",
            sign("PUT", &uppercase, Some(&body), "default"),
        )
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(state.calls.lock().await.len(), 1);
    state.calls.lock().await.clear();
    let response = http
        .put(format!("{url}{uppercase}"))
        .header("authorization", "invalid")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert!(state.calls.lock().await.is_empty());
    configured_options(http, url, id, state).await;
    println!(
        "PASS36 configured HMAC backend HTTP cases: exact public errors/correlation; old/future/repeated signatures; zero provider effects on rejection"
    );
}

async fn configured_options(http: &reqwest::Client, url: &str, id: &str, state: &Provider) {
    for (path, allow) in [
        (format!("/payment-link/{id}"), "DELETE, GET, HEAD, PUT"),
        (format!("/plan/{id}"), "DELETE, GET, HEAD"),
        (format!("/subscription/{id}"), "DELETE, GET, HEAD, PUT"),
        (format!("/checkout-session/{id}"), "GET, HEAD"),
        ("/payment-link".to_owned(), "GET, HEAD, POST"),
    ] {
        for suffix in ["", "/"] {
            let response = http
                .request(reqwest::Method::OPTIONS, format!("{url}{path}{suffix}"))
                .header("authorization", "invalid")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200, "{path}{suffix}");
            assert_eq!(response.headers()["allow"], allow);
            assert_eq!(response.headers()["content-type"], "text/plain");
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert_eq!(response.text().await.unwrap(), allow);
            assert!(state.calls.lock().await.is_empty());
        }
    }
    let response = http
        .request(
            reqwest::Method::OPTIONS,
            format!("{url}/billing-portal/{id}"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 405);
    assert!(state.calls.lock().await.is_empty());
    println!(
        "PASS11 configured OPTIONS HTTP cases: all four HMAC families, aliases/root, no auth/provider effects; Portal stays native"
    );
}
