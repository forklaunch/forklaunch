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
use forklaunch_billing_base::services::{BaseBillingService, BillingKind, BillingStore, Scope};
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
async fn provider(State(state): State<Provider>, method: Method, uri: Uri) -> Response {
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
#[ignore = "owned restricted PostgreSQL and configured cache GET HTTP"]
async fn configured_cached_reads_backend() {
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
    let base = BaseBillingService::new(f.store.clone());
    let backend =
        BillingBackend::stripe((*f.store).clone(), client).with_configured_cached_reads(true);
    let auth = HmacJwksAuth::new(
        KEY.to_vec(),
        RemoteJwks::new(RemoteJwksConfig::new("http://127.0.0.1:9/jwks")).unwrap(),
        Validation::new(Algorithm::RS256),
    )
    .unwrap();
    let router = BillingHttp::new(backend, auth, ServiceInfo::new("billing", "1")).router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router.into_router_with_default_mcp().unwrap())
            .await
            .unwrap();
    });
    let http = reqwest::Client::new();
    for checkout in [false, true] {
        let id = Uuid::new_v4().to_string();
        let kind = if checkout {
            BillingKind::CheckoutSession
        } else {
            BillingKind::PaymentLink
        };
        let evidence = if checkout {
            json!({"id":"cs_owned","object":"checkout.session","mode":"subscription","customer":null,"livemode":false})
        } else {
            json!({"id":"plink_owned","object":"payment_link","livemode":false})
        };
        let mut cached = json!({"id":id,"amount":12,"currency":"USD","status":"PENDING","paymentMethods":["card"],"providerFields":evidence});
        if checkout {
            cached["customerId"] = json!("local-party");
            cached["expiresAt"] = json!("2026-10-20T00:00:00Z");
        }
        base.store()
            .insert(Scope::Cache, kind, cached.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        let path = format!(
            "/{}/{}",
            if checkout {
                "checkout-session"
            } else {
                "payment-link"
            },
            id
        );
        *state.response.lock().await = evidence.clone();
        state.calls.lock().await.clear();
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", sign("GET", &path, None, "default"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["id"], id);
        assert_eq!(value["stripeFields"], evidence);
        assert_eq!(state.calls.lock().await.len(), 1);
        assert_eq!(
            base.store().get(Scope::Cache, kind, &id).await.unwrap(),
            Some(cached.clone())
        );
        // Owner-seeded legacy cache scalars exercise the real decrypt/validate
        // boundary. The configured read normalizes a copy, never the ciphertext.
        let fields = if checkout {
            vec!["expiresAt", "createdAt", "updatedAt"]
        } else {
            vec!["createdAt", "updatedAt"]
        };
        for field in fields {
            for (input, expected) in [
                (json!(0), Some(0)),
                (json!(1.9), Some(1)),
                (json!(-1.9), Some(-1)),
                (json!("2026-01-01"), Some(1_767_225_600_000)),
                (json!("+010000-01-01T00:00:00Z"), Some(253_402_300_800_000)),
                (json!(null), None),
                (json!(8_640_000_000_000_000_i64), None),
            ] {
                let mut tx = f.owner.begin().await.unwrap();
                sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let original:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id='portal-a' AND scope='cache' AND kind=$1 AND id=$2").bind(kind.as_str()).bind(&id).fetch_one(&mut *tx).await.unwrap();
                let modified:sqlx::types::Json<Value>=sqlx::query_scalar("UPDATE fl_billing.records SET data=jsonb_set(data,ARRAY[$3::text],$4) WHERE tenant_id='portal-a' AND scope='cache' AND kind=$1 AND id=$2 RETURNING data").bind(kind.as_str()).bind(&id).bind(field).bind(sqlx::types::Json(input.clone())).fetch_one(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
                if input.is_number() {
                    assert!(
                        base.store().get(Scope::Cache, kind, &id).await.is_err(),
                        "generic typed read unchanged"
                    );
                }
                state.calls.lock().await.clear();
                let response = http
                    .get(format!("{url}{path}"))
                    .header("authorization", sign("GET", &path, None, "default"))
                    .send()
                    .await
                    .unwrap();
                if let Some(expected) = expected {
                    assert_eq!(response.status(), 200, "{field}/{input}");
                    let body: Value = response.json().await.unwrap();
                    assert_eq!(
                        serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
                            body[field].clone()
                        )
                        .unwrap()
                        .timestamp_millis(),
                        expected
                    );
                    assert_eq!(state.calls.lock().await.len(), 1);
                } else {
                    assert_eq!(response.status(), 500);
                    assert!(state.calls.lock().await.is_empty());
                }
                let mut tx = f.owner.begin().await.unwrap();
                sqlx::query("SELECT set_config('forklaunch.tenant_id','portal-a',true)")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let after:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id='portal-a' AND scope='cache' AND kind=$1 AND id=$2").bind(kind.as_str()).bind(&id).fetch_one(&mut *tx).await.unwrap();
                assert_eq!(after, modified);
                sqlx::query("UPDATE fl_billing.records SET data=$3 WHERE tenant_id='portal-a' AND scope='cache' AND kind=$1 AND id=$2").bind(kind.as_str()).bind(&id).bind(original).execute(&mut *tx).await.unwrap();
                tx.commit().await.unwrap();
            }
        }
        if !checkout {
            configured_list_matrix(f, &state, &http, &url, &id, &cached).await;
        }
        for status in [404, 429, 500, 599] {
            *state.status.lock().await = Some(status);
            for correlation in [None, Some("cache-status"), Some("")] {
                let mut request = http
                    .get(format!("{url}{path}"))
                    .header("authorization", sign("GET", &path, None, "default"));
                if let Some(correlation) = correlation {
                    request = request.header("x-correlation-id", correlation);
                }
                let response = request.send().await.unwrap();
                let expected = if status == 599 { 500 } else { status };
                assert_eq!(
                    response.status().as_u16(),
                    expected,
                    "family={checkout} provider={status} correlation={correlation:?} calls={:?}",
                    state.calls.lock().await
                );
                let observed = response
                    .headers()
                    .get("x-correlation-id")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                if correlation.is_some_and(|value| !value.is_empty()) {
                    assert_eq!(observed, correlation.unwrap());
                } else {
                    assert!(Uuid::parse_str(&observed).is_ok());
                }
                assert_eq!(
                    response.text().await.unwrap(),
                    format!(
                        "{}:\n\nCorrelation id: {observed}",
                        if expected >= 500 {
                            "Internal server error"
                        } else {
                            "Request could not be processed"
                        }
                    )
                );
            }
        }
        *state.status.lock().await = None;
        // Real restricted storage failure stays a storage error internally but
        // receives the selected application's final HTTP error formatting.
        let role: String = sqlx::query_scalar("SELECT current_user")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "REVOKE SELECT ON fl_billing.records FROM {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        state.calls.lock().await.clear();
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", sign("GET", &path, None, "default"))
            .header("x-correlation-id", "storage")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(
            response.text().await.unwrap(),
            "Internal server error:\n\nCorrelation id: storage"
        );
        assert!(state.calls.lock().await.is_empty());
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "GRANT SELECT ON fl_billing.records TO {role}"
        )))
        .execute(&f.owner)
        .await
        .unwrap();
        *state.response.lock().await = json!({"id":"wrong"});
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", sign("GET", &path, None, "default"))
            .header("x-correlation-id", "cached-read")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
        assert!(response.text().await.unwrap().contains("cached-read"));
        cached["providerFields"] = json!({});
        base.store()
            .insert(Scope::Cache, kind, cached.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        state.calls.lock().await.clear();
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", sign("GET", &path, None, "default"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 500);
        assert!(state.calls.lock().await.is_empty());
    }
    state.calls.lock().await.clear();
    for (path, expected) in [
        (format!("/payment-link/{}", Uuid::new_v4()), 500),
        ("/payment-link/%0A".into(), 400),
    ] {
        let response = http
            .get(format!("{url}{path}"))
            .header("authorization", sign("GET", &path, None, "default"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        assert!(state.calls.lock().await.is_empty());
    }
    server.abort();
    provider_task.abort();
}

async fn list_response(http: &reqwest::Client, url: &str, ids: Option<Value>) -> reqwest::Response {
    let mut target = reqwest::Url::parse(&format!("{url}/payment-link")).unwrap();
    if let Some(ids) = ids {
        target
            .query_pairs_mut()
            .append_pair("ids", &ids.to_string());
    }
    let path = target.as_str()[url.len()..].to_owned();
    http.get(target)
        .header("authorization", sign("GET", &path, None, "default"))
        .header("x-correlation-id", "list-test")
        .send()
        .await
        .unwrap()
}
#[allow(clippy::too_many_lines)]
async fn configured_list_matrix(
    f: &Fixture,
    state: &Provider,
    http: &reqwest::Client,
    url: &str,
    id: &str,
    cached: &Value,
) {
    let second = Uuid::new_v4().to_string();
    let mut other = cached.clone();
    other["id"] = json!(second);
    f.store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            other.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let mut evidence = cached["providerFields"].clone();
    evidence["active"] = json!(true);
    *state.response.lock().await = evidence.clone();
    for ids in [None, Some(json!([])), Some(json!([second, id, second]))] {
        state.calls.lock().await.clear();
        let expected = if ids
            .as_ref()
            .is_some_and(|ids| ids.as_array().unwrap().len() == 3)
        {
            vec![second.as_str(), id, second.as_str()]
        } else {
            vec![id, second.as_str()]
        };
        let response = list_response(http, url, ids).await;
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            body.as_array()
                .unwrap()
                .iter()
                .map(|value| value["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            body.as_array()
                .unwrap()
                .iter()
                .all(|value| value["status"] == "PENDING" && value["amount"] == 12.0)
        );
        assert_eq!(state.calls.lock().await.len(), expected.len());
        assert!(
            state
                .calls
                .lock()
                .await
                .iter()
                .all(|call| call == "GET /v1/payment_links/plink_owned")
        );
    }
    let mut mcp_url = reqwest::Url::parse(&format!("{url}/payment-link")).unwrap();
    mcp_url
        .query_pairs_mut()
        .append_pair("ids", &json!([second, id, second]).to_string());
    let mcp_path = &mcp_url.as_str()[url.len()..];
    state.calls.lock().await.clear();
    let response=http.post(format!("{url}/mcp")).header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"listPaymentLinks","arguments":{"query":{"ids":[second,id,second]},"headers":{"authorization":sign("GET",mcp_path,None,"default")}}}})).send().await.unwrap();
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["result"]["isError"], false, "{body}");
    assert_eq!(state.calls.lock().await.len(), 3);
    for ids in [
        json!([id, Uuid::new_v4().to_string()]),
        json!([" "]),
        json!(vec![id; 101]),
    ] {
        state.calls.lock().await.clear();
        let response = list_response(http, url, Some(ids)).await;
        assert_eq!(response.status(), 500);
        assert_eq!(
            response.text().await.unwrap(),
            "Internal server error:\n\nCorrelation id: list-test"
        );
        assert!(state.calls.lock().await.is_empty());
    }
    other["providerFields"] = json!({"id":"plink_owned","object":"wrong","livemode":false});
    f.store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            other.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    state.calls.lock().await.clear();
    assert_eq!(
        list_response(http, url, Some(json!([id, second])))
            .await
            .status(),
        500
    );
    assert!(state.calls.lock().await.is_empty());
    other["providerFields"] = cached["providerFields"].clone();
    f.store
        .insert(
            Scope::Cache,
            BillingKind::PaymentLink,
            other.clone(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    evidence["active"] = json!(false);
    *state.response.lock().await = evidence.clone();
    assert_eq!(
        list_response(http, url, Some(json!([id, second])))
            .await
            .json::<Value>()
            .await
            .unwrap(),
        json!([])
    );
    for active in [Value::Null, json!("true"), json!(1)] {
        evidence["active"] = active;
        *state.response.lock().await = evidence.clone();
        state.calls.lock().await.clear();
        assert_eq!(
            list_response(http, url, Some(json!([id, second])))
                .await
                .status(),
            500
        );
        assert_eq!(state.calls.lock().await.len(), 1);
    }
    for status in [404, 429, 500] {
        *state.status.lock().await = Some(status);
        let response = list_response(http, url, Some(json!([id]))).await;
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(
            response.text().await.unwrap(),
            format!(
                "{}:\n\nCorrelation id: list-test",
                if status == 500 {
                    "Internal server error"
                } else {
                    "Request could not be processed"
                }
            )
        );
    }
    *state.status.lock().await = None;
    // Enumerated count admission must happen before decrypt/read-audit: revoke
    // INSERT, proving a within-limit read fails while excess keys return the
    // configured bound error without trying to audit either row.
    let role: String = sqlx::query_scalar("SELECT current_user")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "REVOKE INSERT ON fl_billing.sensitive_read_audit FROM {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    for ids in [None, Some(&[][..])] {
        assert_eq!(
            f.store
                .list_configured_cached(ids, 1)
                .await
                .unwrap_err()
                .code,
            "billing_configured_mapper_error"
        );
    }
    assert_eq!(
        f.store
            .list_configured_cached(None, 2)
            .await
            .unwrap_err()
            .code,
        "billing_storage"
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "GRANT INSERT ON fl_billing.sensitive_read_audit TO {role}"
    )))
    .execute(&f.owner)
    .await
    .unwrap();
    assert_eq!(
        f.store
            .get(Scope::Cache, BillingKind::PaymentLink, id)
            .await
            .unwrap(),
        Some(cached.clone())
    );
    assert_eq!(
        f.store
            .get(Scope::Cache, BillingKind::PaymentLink, &second)
            .await
            .unwrap(),
        Some(other)
    );
    f.store
        .delete(Scope::Cache, BillingKind::PaymentLink, &second)
        .await
        .unwrap();
    *state.response.lock().await = cached["providerFields"].clone();
}
