use axum::{
    Json, Router,
    extract::{Form, Path, Query as HttpQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, InMemoryEcommerceStore, Query, RecordKind},
};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentService, PaymentStatus,
};
use forklaunch_ecommerce_stripe::{StripeClient, StripeConnect, StripePaymentService};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Default)]
struct Fixture {
    calls: Arc<Mutex<Vec<serde_json::Value>>>,
    details: Arc<Mutex<Option<serde_json::Value>>>,
}
async fn create(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Form(fields): Form<HashMap<String, String>>,
) -> Response {
    assert_eq!(headers["authorization"], "Bearer owned-test-stripe-secret");
    state.calls.lock().unwrap().push(json!({"method":"POST","fields":fields,"account":headers.get("stripe-account").map(|v|v.to_str().unwrap())}));
    match fields["metadata[orderId]"].as_str() {
        "error" => (
            StatusCode::BAD_GATEWAY,
            "private provider body: owned-test-stripe-secret",
        )
            .into_response(),
        "redirect" => (StatusCode::FOUND, [("location", "/trap")]).into_response(),
        "large" => Json(json!({"id":"pi_large","client_secret":"x".repeat(4000)})).into_response(),
        "slow" => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Json(json!({"id":"pi_slow"})).into_response()
        }
        "no-secret" => Json(json!({"id":"pi_no_secret","client_secret":null})).into_response(),
        _ => {
            Json(json!({"id":"pi_owned","client_secret":"ephemeral-create-secret"})).into_response()
        }
    }
}
async fn retrieve(
    State(state): State<Fixture>,
    Path(id): Path<String>,
    headers: HeaderMap,
    HttpQuery(query): HttpQuery<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    assert_eq!(headers["authorization"], "Bearer owned-test-stripe-secret");
    state.calls.lock().unwrap().push(json!({"method":"GET","id":id,"account":headers.get("stripe-account").map(|v|v.to_str().unwrap())}));
    if let Some(details) = state.details.lock().unwrap().clone() {
        assert_eq!(
            query.get("expand[]").map(String::as_str),
            Some("latest_charge")
        );
        return Json(details);
    }
    Json(json!({"id":id,"client_secret":"ephemeral-resume-secret"}))
}
fn request(id: &str, order: &str, amount: i64) -> CreatePaymentDto {
    CreatePaymentDto {
        id: Some(id.into()),
        order_id: order.into(),
        amount_cents: amount,
        currency: "usd".into(),
    }
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_stripe_connect_resume_secrets_failures_bounds_and_persistence() {
    let state = Fixture::default();
    let app = Router::new()
        .route("/v1/payment_intents", post(create))
        .route("/v1/payment_intents/{id}", get(retrieve))
        .route(
            "/trap",
            get(|| async {
                panic!("Provider redirects must never be followed");
                #[allow(unreachable_code)]
                "trap"
            }),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let store = Arc::new(InMemoryEcommerceStore::default());
    let base = BaseEcommerceService::new(Arc::clone(&store));
    let client = StripeClient::with_transport(
        "owned-test-stripe-secret".into(),
        ProviderHttpClient::new(&origin, Duration::from_secs(2), 1024).unwrap(),
    )
    .unwrap();
    assert!(!format!("{client:?}").contains("owned-test-stripe-secret"));
    let service = StripePaymentService::new(base.clone(), client.clone())
        .with_connect(StripeConnect::new("acct_merchant".into(), 250).unwrap());
    let created = service
        .create_payment(request("pay", "order", 5350))
        .await
        .unwrap();
    assert_eq!(
        created.client_secret.as_deref(),
        Some("ephemeral-create-secret")
    );
    assert_eq!(created.provider_ref.as_deref(), Some("pi_owned"));
    assert!(!format!("{created:?}").contains("ephemeral-create-secret"));
    let wire = serde_json::to_value(&created).unwrap();
    assert_eq!(wire["clientSecret"], "ephemeral-create-secret");
    assert_eq!(wire["id"], "pay");
    assert!(wire.get("payment").is_none());
    let persisted =
        serde_json::to_value(store.find(Query::all(RecordKind::Payment)).await.unwrap())
            .unwrap()
            .to_string();
    assert!(!persisted.contains("Secret"));
    assert!(!persisted.contains("ephemeral"));
    let resumed = service.resume_payment("pay").await.unwrap();
    assert_eq!(
        resumed.client_secret.as_deref(),
        Some("ephemeral-resume-secret")
    );
    assert_eq!(resumed.id, "pay");
    assert_eq!(
        service.get_payment("pay").await.unwrap().status,
        PaymentStatus::Pending
    );
    {
        let calls = state.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["fields"]["application_fee_amount"], "134");
        assert_eq!(calls[0]["fields"]["metadata[orderId]"], "order");
        for call in calls.iter() {
            assert_eq!(call["account"], "acct_merchant");
        }
    }
    let evidence = json!({"id":"pi_owned","object":"payment_intent","status":"succeeded",
        "amount":5350,"amount_received":5350,"currency":"usd","metadata":{"orderId":"order"},
        "client_secret":"must-not-persist",
        "latest_charge":{"id":"ch_owned","object":"charge","payment_intent":"pi_owned",
            "status":"succeeded","paid":true,"captured":true,"refunded":false,"disputed":false,
            "amount":5350,"amount_captured":5350,"amount_refunded":0,"currency":"usd"}});
    *state.details.lock().unwrap() = Some(evidence.clone());
    let actor = forklaunch_core::domain::AuditContext::actor("operator", "stripe-recovery");
    assert_eq!(
        service
            .reconcile_captured_payment("pay", actor.clone())
            .await
            .unwrap()
            .status,
        PaymentStatus::Succeeded
    );
    let audit = store.payment_reconciliation_audits().unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].evidence.capture_id, "ch_owned");
    assert!(!format!("{audit:?}").contains("must-not-persist"));
    for (pointer, replacement) in [
        ("/id", json!("other")),
        ("/status", json!("processing")),
        ("/status", json!("requires_capture")),
        ("/object", json!("charge")),
        ("/amount", json!(1)),
        ("/amount_received", json!(1)),
        ("/currency", json!("eur")),
        ("/metadata/orderId", json!("other-order")),
        ("/latest_charge", json!("ch_unexpanded")),
        ("/latest_charge/id", json!("")),
        ("/latest_charge/payment_intent", json!("other")),
        ("/latest_charge/status", json!("failed")),
        ("/latest_charge/paid", json!(false)),
        ("/latest_charge/captured", json!(false)),
        ("/latest_charge/refunded", json!(true)),
        ("/latest_charge/disputed", json!(true)),
        ("/latest_charge/amount_refunded", json!(1)),
        ("/latest_charge/amount_captured", json!(1)),
        ("/latest_charge/amount", json!(1)),
        ("/latest_charge/currency", json!("eur")),
    ] {
        let mut invalid = evidence.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        *state.details.lock().unwrap() = Some(invalid);
        let error = service
            .reconcile_captured_payment("pay", actor.clone())
            .await
            .unwrap_err();
        assert!(!error.message.contains("must-not-persist"));
    }
    assert_eq!(store.payment_reconciliation_audits().unwrap().len(), 1);
    base.create_payment_with_reference(
        request("canceled", "canceled-order", 5350),
        Some("pi_canceled".into()),
    )
    .await
    .unwrap();
    assert!(base.claim_for_confirmation("pi_canceled").await.unwrap());
    let canceled = json!({"id":"pi_canceled","object":"payment_intent","status":"canceled",
        "canceled_at":1_700_000_000,"amount":5350,"amount_received":0,"amount_capturable":0,
        "currency":"usd","metadata":{"orderId":"canceled-order"},"latest_charge":null,
        "client_secret":"must-not-persist"});
    for (pointer, replacement) in [
        ("/id", json!("other")),
        ("/object", json!("charge")),
        ("/status", json!("processing")),
        ("/status", json!("requires_capture")),
        ("/status", json!("succeeded")),
        ("/canceled_at", json!(null)),
        ("/canceled_at", json!(0)),
        ("/canceled_at", json!("1700000000")),
        ("/amount", json!(1)),
        ("/amount_received", json!(1)),
        ("/amount_capturable", json!(1)),
        ("/currency", json!("eur")),
        ("/metadata/orderId", json!("other")),
        ("/latest_charge", json!("ch_prior")),
        ("/latest_charge", json!({"paid":false})),
    ] {
        let mut invalid = canceled.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        *state.details.lock().unwrap() = Some(invalid);
        assert!(
            service
                .reconcile_canceled_payment("canceled", actor.clone())
                .await
                .is_err()
        );
    }
    let mut missing_charge = canceled.clone();
    missing_charge
        .as_object_mut()
        .unwrap()
        .remove("latest_charge");
    *state.details.lock().unwrap() = Some(missing_charge);
    assert!(
        service
            .reconcile_canceled_payment("canceled", actor.clone())
            .await
            .is_err()
    );
    assert_eq!(
        base.get_payment("canceled").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert!(store.payment_cancellation_audits().unwrap().is_empty());
    *state.details.lock().unwrap() = Some(canceled.clone());
    let repaired = service
        .reconcile_canceled_payment("canceled", actor.clone())
        .await
        .unwrap();
    assert_eq!(repaired.status, PaymentStatus::Failed);
    assert_eq!(repaired.provider_ref.as_deref(), Some("pi_canceled"));
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "pi_canceled".into()
            })
            .await
            .is_err()
    );
    let cancellations = store.payment_cancellation_audits().unwrap();
    assert_eq!(cancellations.len(), 1);
    assert_eq!(cancellations[0].evidence.canceled_at, Some(1_700_000_000));
    assert!(!format!("{cancellations:?}").contains("must-not-persist"));
    // A terminal canceled intent may retain its last failed attempt. Verify
    // every money/status/identity field before recording the same durable repair.
    base.create_payment_with_reference(
        request("failed-charge", "canceled-order", 5350),
        Some("pi_failed_charge".into()),
    )
    .await
    .unwrap();
    assert!(
        base.claim_for_confirmation("pi_failed_charge")
            .await
            .unwrap()
    );
    let mut with_failed_charge = canceled;
    with_failed_charge["id"] = json!("pi_failed_charge");
    with_failed_charge["latest_charge"] = json!({
        "id":"ch_failed", "object":"charge", "payment_intent":"pi_failed_charge",
        "status":"failed", "paid":false, "captured":false, "refunded":false,
        "disputed":false, "amount":5350, "amount_captured":0, "amount_refunded":0,
        "currency":"usd"
    });
    for (field, replacement) in [
        ("id", json!("")),
        ("object", json!("payment_intent")),
        ("payment_intent", json!("pi_other")),
        ("status", json!("pending")),
        ("status", json!("succeeded")),
        ("paid", json!(true)),
        ("captured", json!(true)),
        ("refunded", json!(true)),
        ("disputed", json!(true)),
        ("amount", json!(5351)),
        ("amount_captured", json!(1)),
        ("amount_refunded", json!(1)),
        ("currency", json!("eur")),
        ("amount_captured", json!("0")),
    ] {
        let mut invalid = with_failed_charge.clone();
        invalid["latest_charge"][field] = replacement;
        *state.details.lock().unwrap() = Some(invalid);
        assert!(
            service
                .reconcile_canceled_payment("failed-charge", actor.clone())
                .await
                .is_err()
        );
    }
    for field in with_failed_charge["latest_charge"]
        .as_object()
        .unwrap()
        .keys()
    {
        let mut invalid = with_failed_charge.clone();
        invalid["latest_charge"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        *state.details.lock().unwrap() = Some(invalid);
        assert!(
            service
                .reconcile_canceled_payment("failed-charge", actor.clone())
                .await
                .is_err()
        );
    }
    // Failed/unpaid charge evidence cannot turn a still-retryable intent terminal.
    let mut pending = with_failed_charge.clone();
    pending["status"] = json!("requires_payment_method");
    *state.details.lock().unwrap() = Some(pending);
    assert!(
        service
            .reconcile_canceled_payment("failed-charge", actor.clone())
            .await
            .is_err()
    );
    assert_eq!(
        base.get_payment("failed-charge").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert_eq!(store.payment_cancellation_audits().unwrap().len(), 1);
    *state.details.lock().unwrap() = Some(with_failed_charge);
    let repaired = service
        .reconcile_canceled_payment("failed-charge", actor.clone())
        .await
        .unwrap();
    assert_eq!(repaired.status, PaymentStatus::Failed);
    assert_eq!(repaired.provider_ref.as_deref(), Some("pi_failed_charge"));
    assert!(
        base.claim_for_confirmation("pi_failed_charge")
            .await
            .is_err()
    );
    base.release_confirmation_claim("pi_failed_charge")
        .await
        .unwrap();
    assert_eq!(
        base.get_payment("failed-charge").await.unwrap().status,
        PaymentStatus::Failed
    );
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "pi_failed_charge".into()
            })
            .await
            .is_err()
    );
    let audit = store.payment_cancellation_audits().unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].previous_status, PaymentStatus::Succeeded);
    assert_eq!(audit[1].evidence.provider_ref, "pi_failed_charge");
    assert_eq!(audit[1].actor, actor);
    assert!(
        state.calls.lock().unwrap()[2..]
            .iter()
            .all(|call| call["method"] == "GET" && call["account"] == "acct_merchant")
    );
    *state.details.lock().unwrap() = None;
    base.create_payment(request("providerless", "other", 100))
        .await
        .unwrap();
    let count = state.calls.lock().unwrap().len();
    assert!(
        service
            .resume_payment("providerless")
            .await
            .unwrap()
            .client_secret
            .is_none()
    );
    assert_eq!(state.calls.lock().unwrap().len(), count);
    service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "pi_owned".into(),
        })
        .await
        .unwrap();
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "pi_owned".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        service.get_payment("pay").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    let direct = StripePaymentService::new(
        BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default())),
        client,
    );
    assert!(
        direct
            .create_payment(request("none", "no-secret", 1))
            .await
            .unwrap()
            .client_secret
            .is_none()
    );
    {
        let calls = state.calls.lock().unwrap();
        let last = calls.last().unwrap();
        assert!(last["account"].is_null());
        assert!(last["fields"].get("application_fee_amount").is_none());
    }
    for order in ["error", "redirect", "large"] {
        let count = state.calls.lock().unwrap().len();
        let error = direct
            .create_payment(request(order, order, 100))
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("private provider"));
        assert!(!format!("{error:?}").contains("owned-test-stripe-secret"));
        assert!(direct.get_payment(order).await.is_err());
        assert_eq!(
            state.calls.lock().unwrap().len(),
            count + 1,
            "Provider failures must not retry"
        );
    }
    let short = StripeClient::with_transport(
        "owned-test-stripe-secret".into(),
        ProviderHttpClient::new(&origin, Duration::from_millis(10), 1024).unwrap(),
    )
    .unwrap();
    assert!(
        short
            .create_intent(&request("slow", "slow", 100), None)
            .await
            .is_err()
    );
    task.abort();
    let _ = task.await;
}
#[test]
fn connect_fees_round_and_cap_without_overflow() {
    for (amount, bps, expected) in [
        (5350, 250, 134),
        (1, 9999, 0),
        (0, 9999, 0),
        (100, 0, 0),
        (101, 5000, 51),
        (100, 9999, 99),
        (i64::MAX, 9999, 9_222_449_699_651_090_329),
    ] {
        assert_eq!(
            StripeConnect::new("acct_test".into(), bps)
                .unwrap()
                .fee_cents(amount)
                .unwrap(),
            expected
        );
    }
    for bps in [10000, 10001, u16::MAX] {
        assert!(StripeConnect::new("acct_test".into(), bps).is_err());
    }
    assert!(StripeConnect::new("acct\ninvalid".into(), 0).is_err());
    assert!(
        StripeConnect::new("acct_test".into(), 1)
            .unwrap()
            .fee_cents(-1)
            .is_err()
    );
}
