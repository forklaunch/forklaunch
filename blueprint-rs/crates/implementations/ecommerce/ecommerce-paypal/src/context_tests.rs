use super::*;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Clone)]
struct Fixture {
    token: Arc<Mutex<Value>>,
    order: Arc<Mutex<Value>>,
    calls: Arc<AtomicUsize>,
    denied: Arc<AtomicUsize>,
}
fn order() -> Value {
    serde_json::json!({"id":"ORDER_owned","intent":"CAPTURE","status":"COMPLETED","purchase_units":[{"reference_id":"local-order","payee":{"merchant_id":"Q7C9N6S4YRQXG","email_address":"private@example.invalid"},"amount":{"currency_code":"USD","value":"1.05"}}],"payer":{"private":"never-return"}})
}
async fn token(
    State(s): State<Fixture>,
    headers: HeaderMap,
    body: String,
) -> (StatusCode, Json<Value>) {
    s.calls.fetch_add(1, Ordering::SeqCst);
    assert_eq!(
        headers["authorization"],
        "Basic b3duZWQtY2xpZW50Om93bmVkLXNlY3JldA=="
    );
    assert_eq!(body, "grant_type=client_credentials");
    assert!(!headers.contains_key("paypal-auth-assertion"));
    assert!(!headers.contains_key("paypal-partner-attribution-id"));
    (
        if s.denied.load(Ordering::SeqCst) == 1 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        },
        Json(s.token.lock().unwrap().clone()),
    )
}
async fn read(State(s): State<Fixture>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    s.calls.fetch_add(1, Ordering::SeqCst);
    assert_eq!(headers["authorization"], "Bearer owned-token");
    assert!(!headers.contains_key("paypal-auth-assertion"));
    assert!(!headers.contains_key("paypal-partner-attribution-id"));
    (
        if s.denied.load(Ordering::SeqCst) == 2 {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::OK
        },
        Json(s.order.lock().unwrap().clone()),
    )
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn scoped_resource_auth_shape_generation_environment_and_expiry() {
    let valid_token = serde_json::json!({"access_token":"owned-token","token_type":"Bearer","expires_in":300,"app_id":"not-a-merchant"});
    let state = Fixture {
        token: Arc::new(Mutex::new(valid_token.clone())),
        order: Arc::new(Mutex::new(order())),
        calls: Arc::new(AtomicUsize::new(0)),
        denied: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/v1/oauth2/token", post(token))
        .route("/v2/checkout/orders/ORDER_owned", get(read))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let http = ProviderHttpClient::new(&origin, Duration::from_secs(2), 4096).unwrap();
    let legacy =
        PaypalClient::with_transport("owned-client".into(), "owned-secret".into(), http.clone())
            .unwrap();
    let client = PaypalClient::with_trusted_environment_transport(
        "owned-client".into(),
        "owned-secret".into(),
        PaypalEnvironment::Sandbox,
        http.clone(),
    )
    .unwrap();
    for (candidate, env, merchant) in [
        (&legacy, PaypalEnvironment::Sandbox, "Q7C9N6S4YRQXG"),
        (&client, PaypalEnvironment::Live, "Q7C9N6S4YRQXG"),
        (&client, PaypalEnvironment::Sandbox, "email@example.invalid"),
        (&client, PaypalEnvironment::Sandbox, "Q7C9N6S4YRQX0"),
    ] {
        assert!(
            candidate
                .verify_order_context(env, merchant, "ORDER_owned")
                .await
                .is_err()
        );
    }
    assert_eq!(state.calls.load(Ordering::SeqCst), 0);
    let mut context = client
        .verify_order_context(PaypalEnvironment::Sandbox, "Q7C9N6S4YRQXG", "ORDER_owned")
        .await
        .unwrap();
    assert_eq!(context.reference_id(), "local-order");
    assert_eq!(context.merchant_id(), "Q7C9N6S4YRQXG");
    assert_eq!(context.observation().amount_cents, 105);
    assert!(context.ensure_current(&client.clone()).is_ok());
    assert!(
        !serde_json::to_string(context.observation())
            .unwrap()
            .contains("private")
    );
    let foreign = PaypalClient::with_trusted_environment_transport(
        "owned-client".into(),
        "owned-secret".into(),
        PaypalEnvironment::Sandbox,
        http,
    )
    .unwrap();
    assert!(context.ensure_current(&foreign).is_err());
    context.started_at = Instant::now().checked_sub(Duration::from_secs(6)).unwrap();
    assert!(context.ensure_current(&client).is_err());
    for (key, value) in [
        ("access_token", Value::Null),
        ("access_token", serde_json::json!("")),
        ("access_token", serde_json::json!("bad token")),
        ("token_type", serde_json::json!("Basic")),
        ("expires_in", serde_json::json!(0)),
        ("expires_in", serde_json::json!(-1)),
        ("expires_in", serde_json::json!("300")),
    ] {
        let mut bad = valid_token.clone();
        bad[key] = value;
        *state.token.lock().unwrap() = bad;
        let before = state.calls.load(Ordering::SeqCst);
        assert!(
            client
                .verify_order_context(PaypalEnvironment::Sandbox, "Q7C9N6S4YRQXG", "ORDER_owned")
                .await
                .is_err()
        );
        assert_eq!(state.calls.load(Ordering::SeqCst), before + 1);
    }
    *state.token.lock().unwrap() = valid_token;
    for denied in [1, 2] {
        state.denied.store(denied, Ordering::SeqCst);
        assert!(
            client
                .verify_order_context(PaypalEnvironment::Sandbox, "Q7C9N6S4YRQXG", "ORDER_owned")
                .await
                .is_err()
        );
    }
    state.denied.store(0, Ordering::SeqCst);
    for (pointer, value) in [
        ("/id", serde_json::json!("other")),
        ("/intent", serde_json::json!("AUTHORIZE")),
        ("/status", serde_json::json!("unknown")),
        ("/purchase_units/0/payee/merchant_id", Value::Null),
        (
            "/purchase_units/0/payee/merchant_id",
            serde_json::json!("J8NCBBULP5JH2"),
        ),
        ("/purchase_units/0/amount/value", serde_json::json!("1.005")),
        (
            "/purchase_units/0/amount/currency_code",
            serde_json::json!("usd"),
        ),
        ("/purchase_units/0/reference_id", serde_json::json!("")),
        ("/purchase_units", serde_json::json!([])),
    ] {
        let mut bad = order();
        *bad.pointer_mut(pointer).unwrap() = value;
        *state.order.lock().unwrap() = bad;
        assert!(
            client
                .verify_order_context(PaypalEnvironment::Sandbox, "Q7C9N6S4YRQXG", "ORDER_owned")
                .await
                .is_err()
        );
    }
    server.abort();
}
#[test]
fn fixed_origins_and_primary_merchant_format() {
    assert_eq!(
        PaypalEnvironment::Sandbox.origin(),
        "https://api-m.sandbox.paypal.com"
    );
    assert_eq!(PaypalEnvironment::Live.origin(), "https://api-m.paypal.com");
    assert!(!PaypalEnvironment::Sandbox.livemode());
    assert!(PaypalEnvironment::Live.livemode());
    for environment in [PaypalEnvironment::Sandbox, PaypalEnvironment::Live] {
        let client =
            PaypalClient::for_environment("client".into(), "secret".into(), environment).unwrap();
        assert_eq!(client.scoped_environment, Some(environment));
    }
    for byte in 0_u8..=127 {
        let mut id = "Q7C9N6S4YRQXG".to_owned();
        id.replace_range(0..1, &char::from(byte).to_string());
        assert_eq!(
            merchant_id(&id),
            matches!(byte,b'2'..=b'9'|b'A'..=b'H'|b'J'..=b'N'|b'P'..=b'Z')
        );
    }
}

#[test]
fn voided_history_is_explicit_private_and_does_not_change_status_observation() {
    let mut valid = order();
    valid["status"] = serde_json::json!("VOIDED");
    valid["purchase_units"][0]["payments"] =
        serde_json::json!({"captures":[],"authorizations":[],"refunds":[]});
    assert!(voided_empty_history(&valid));
    for key in ["captures", "authorizations", "refunds"] {
        for value in [
            Value::Null,
            serde_json::json!({}),
            serde_json::json!([{"id":"prior"}]),
        ] {
            let mut bad = valid.clone();
            bad["purchase_units"][0]["payments"][key] = value;
            assert!(!voided_empty_history(&bad));
            assert!(
                verify_order(
                    &bad,
                    PaypalEnvironment::Sandbox,
                    "Q7C9N6S4YRQXG",
                    "ORDER_owned"
                )
                .is_ok()
            );
        }
        let mut bad = valid.clone();
        bad["purchase_units"][0]["payments"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(!voided_empty_history(&bad));
        assert!(
            verify_order(
                &bad,
                PaypalEnvironment::Sandbox,
                "Q7C9N6S4YRQXG",
                "ORDER_owned"
            )
            .is_ok()
        );
    }
    for status in [
        "COMPLETED",
        "APPROVED",
        "CREATED",
        "SAVED",
        "PAYER_ACTION_REQUIRED",
    ] {
        let mut other = valid.clone();
        other["status"] = serde_json::json!(status);
        assert!(!voided_empty_history(&other));
        assert!(
            verify_order(
                &other,
                PaypalEnvironment::Sandbox,
                "Q7C9N6S4YRQXG",
                "ORDER_owned"
            )
            .is_ok()
        );
    }
}
