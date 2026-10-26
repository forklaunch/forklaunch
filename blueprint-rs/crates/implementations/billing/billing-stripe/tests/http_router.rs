#![cfg(feature = "http")]
use axum::{Json, Router, extract::Form, http::HeaderMap, routing::post};
use forklaunch_axum::module_auth::HmacJwksAuth;
use forklaunch_billing_base::services::{BaseBillingService, postgres::PostgresBillingStore};
use forklaunch_billing_interface::services::PlanService;
use forklaunch_billing_stripe::http::{BillingBackend, BillingHttp};
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

use forklaunch_billing_stripe::client::StripeBillingClient;

use jsonwebtoken::{Algorithm, Validation};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::{collections::HashMap, sync::Arc, time::Duration};
const SECRET: &[u8] = b"ecommerce-fixture-secret";
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
#[ignore = "requires isolated FORKLAUNCH_TEST_POSTGRES_URL owner database"]
#[allow(clippy::too_many_lines)]
async fn persistent_billing_http_mcp_and_provider_startup() {
    let url = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").unwrap();
    let owner = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    PostgresBillingStore::migrate(&owner).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("billing_http_{suffix}");
    let password = uuid::Uuid::new_v4().to_string();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD '{password}';GRANT USAGE ON SCHEMA fl_billing TO {role};GRANT SELECT,INSERT,UPDATE,DELETE ON fl_billing.records TO {role};GRANT INSERT ON fl_billing.sensitive_read_audit TO {role};GRANT SELECT,INSERT ON fl_billing.webhook_receipts TO {role};GRANT SELECT,INSERT,DELETE ON fl_billing.webhook_attempts TO {role};GRANT INSERT ON fl_billing.webhook_reconciliation_audit TO {role};GRANT SELECT,INSERT,DELETE ON fl_billing.platform_event_nonces TO {role};"))).execute(&owner).await.unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options.username(&role).password(&password))
        .await
        .unwrap();
    let keys = Arc::new(FieldEncryptor::new("billing-http-owned-key").unwrap());
    let client = Client::new();
    let provider=Router::new().route("/v1/plans",post(|headers:HeaderMap,Form(fields):Form<HashMap<String,String>>|async move {
        assert_eq!(headers["authorization"],"Bearer sk_owned");assert_eq!(headers["stripe-version"],"2026-08-26.dahlia");assert_eq!(fields["amount"],"1999");Json(json!({"id":"provider_plan","object":"plan"}))
    })).route("/v1/plans/{id}",axum::routing::get(||async {Json(json!({"id":"provider_plan","object":"plan","product":{"id":"prod_owned","object":"product","private":"private-provider-value"}}))}));
    let (provider_base, provider_stop, provider_task) = serve(provider).await;
    for stripe in [false, true] {
        let tenant = TenantContext::new(format!("billing-http-{stripe}-{suffix}")).unwrap();
        let store = PostgresBillingStore::new(
            pool.clone(),
            tenant,
            Arc::clone(&keys),
            AuditContext::system("startup"),
        )
        .await
        .unwrap();
        let other_store = PostgresBillingStore::new(
            pool.clone(),
            TenantContext::new(format!("other-{stripe}-{suffix}")).unwrap(),
            Arc::clone(&keys),
            AuditContext::system("other"),
        )
        .await
        .unwrap();
        let backend = if stripe {
            BillingBackend::stripe(
                store,
                StripeBillingClient::with_transport(
                    "sk_owned".into(),
                    ProviderHttpClient::new(&provider_base, Duration::from_secs(5), 1024 * 1024)
                        .unwrap(),
                )
                .unwrap(),
            )
        } else {
            BillingBackend::base(store)
        };
        let auth = HmacJwksAuth::new(
            SECRET.to_vec(),
            RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
            Validation::new(Algorithm::RS256),
        )
        .unwrap();
        let router = BillingHttp::new(backend, auth, ServiceInfo::new("billing", "1.0.0")).router();
        assert_eq!(
            router.registry().routes().len(),
            if stripe { 35 } else { 33 }
        );
        let (base, stop, task) = serve(router.into_router_with_default_mcp().unwrap()).await;
        assert_eq!(
            client
                .get(format!("{base}/health"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .get(format!("{base}/plan"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            client
                .get(format!("{base}/billing-portal/missing"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let mut body = json!({"name":"prod_owned","price":1999,"cadence":if stripe {"month"}else{"MONTHLY"},"currency":"USD","externalId":"external-owned","billingProvider":if stripe{"stripe"}else{"none"},"active":true});
        if stripe {
            body["stripeFields"] = json!({"metadata":{"note":"private-provider-value"}});
        } else {
            body["providerFields"] = json!({"private":"private-provider-value"});
        }
        let mut invalid = body.clone();
        invalid["currency"] = json!("INVALID");
        assert_eq!(
            call(&client, &base, Method::POST, "/plan", Some(invalid))
                .await
                .status(),
            400
        );
        let plan = value(call(&client, &base, Method::POST, "/plan", Some(body)).await).await;
        let id = plan["id"].as_str().unwrap();
        uuid::Uuid::parse_str(id).unwrap();
        if stripe {
            assert_eq!(plan["externalId"], "provider_plan");
            assert_eq!(plan["stripeFields"]["private"], "private-provider-value");
            assert!(plan.get("providerFields").is_none());
        } else {
            assert_eq!(plan["providerFields"]["private"], "private-provider-value");
        }
        assert!(
            BaseBillingService::new(Arc::new(other_store))
                .get_plan(id)
                .await
                .is_err()
        );
        assert_eq!(
            value(call(&client, &base, Method::GET, &format!("/plan/{id}"), None).await).await["id"],
            id
        );
        let initialized=rpc(&client,&base,"initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "billing");
        assert_eq!(
            rpc(&client, &base, "tools/list", json!({})).await["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            if stripe { 35 } else { 33 }
        );
        let tool=rpc(&client,&base,"tools/call",json!({"name":"listPlans","arguments":{"query":{"ids":[id]},"headers":{"authorization":sign("GET","/plan",None)}}})).await;
        assert_ne!(tool["result"]["isError"], true);
        let output: Value =
            serde_json::from_str(tool["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(output[0]["id"], id);
        let tool = rpc(
            &client,
            &base,
            "tools/call",
            json!({"name":"listPlans","arguments":{}}),
        )
        .await;
        assert_eq!(tool["result"]["isError"], true);
        let (raw,): (String,) = sqlx::query_as(
            "SELECT data::text FROM fl_billing.records WHERE id=$1 AND scope='durable'",
        )
        .bind(id)
        .fetch_one(&owner)
        .await
        .unwrap();
        assert!(!raw.contains("private-provider-value"));
        let (startup_reads,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM fl_billing.sensitive_read_audit WHERE actor_id IS DISTINCT FROM 'service:default' AND record_id=$1",
        )
        .bind(id)
        .fetch_one(&owner)
        .await
        .unwrap();
        assert_eq!(startup_reads, 0);
        stop.send(()).unwrap();
        task.await.unwrap();
    }
    provider_stop.send(()).unwrap();
    provider_task.await.unwrap();
    pool.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP OWNED BY {role};DROP ROLE {role};"
    )))
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
