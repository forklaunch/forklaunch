use axum::{
    Json, Router,
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use forklaunch_ecommerce_base::{BaseEcommerceService, store::InMemoryEcommerceStore};
use forklaunch_ecommerce_interface::{
    ConfirmPaymentDto, CreatePaymentDto, FailPaymentDto, PaymentService, PaymentStatus,
};
use forklaunch_ecommerce_paypal::{PaypalClient, PaypalPaymentService, VerifyWebhookSignature};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Clone, Default)]
struct Fixture {
    calls: Arc<Mutex<Vec<Value>>>,
    captures: Arc<AtomicUsize>,
    fail_capture: Arc<AtomicBool>,
    details: Arc<Mutex<Option<Value>>>,
}
async fn token(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Form(fields): Form<HashMap<String, String>>,
) -> Json<Value> {
    assert_eq!(
        headers["authorization"],
        "Basic b3duZWQtY2xpZW50Om93bmVkLXNlY3JldA=="
    );
    assert_eq!(fields["grant_type"], "client_credentials");
    state
        .calls
        .lock()
        .unwrap()
        .push(json!({"operation":"token"}));
    Json(json!({"access_token":"owned-access-token"}))
}
async fn create(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-access-token");
    state
        .calls
        .lock()
        .unwrap()
        .push(json!({"operation":"create","body":body}));
    Json(json!({"id":body["purchase_units"][0]["reference_id"],"status":"CREATED"}))
}
async fn capture(
    State(state): State<Fixture>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    assert_eq!(headers["authorization"], "Bearer owned-access-token");
    state.captures.fetch_add(1, Ordering::SeqCst);
    state
        .calls
        .lock()
        .unwrap()
        .push(json!({"operation":"capture","id":id}));
    tokio::time::sleep(Duration::from_millis(50)).await;
    if state.fail_capture.swap(false, Ordering::SeqCst) {
        return (
            StatusCode::BAD_GATEWAY,
            "private provider failure owned-secret",
        )
            .into_response();
    }
    Json(json!({"id":id,"status":"COMPLETED"})).into_response()
}
fn completed_details(id: &str) -> Value {
    json!({"id":id,"status":"COMPLETED","intent":"CAPTURE","purchase_units":[{
        "reference_id":id,"amount":{"currency_code":"USD","value":"1.05"},
        "payments":{"captures":[{"id":"CAPTURE_owned","status":"COMPLETED","final_capture":true,
            "amount":{"currency_code":"USD","value":"1.05"}}]}}]})
}
async fn details(
    State(state): State<Fixture>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer owned-access-token");
    state
        .calls
        .lock()
        .unwrap()
        .push(json!({"operation":"retrieve","id":id}));
    Json(
        state
            .details
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| completed_details(&id)),
    )
}
async fn verify(State(state): State<Fixture>, Json(body): Json<Value>) -> Response {
    state
        .calls
        .lock()
        .unwrap()
        .push(json!({"operation":"verify","body":body}));
    match body["webhook_event"]["mode"].as_str().unwrap() {
        "success" => Json(json!({"verification_status":"SUCCESS"})).into_response(),
        "bad-shape" => Json(json!({"verification_status":42})).into_response(),
        "status-failure" => (StatusCode::UNAUTHORIZED, "not verified").into_response(),
        _ => Json(json!({"verification_status":"FAILURE"})).into_response(),
    }
}
fn request(id: &str) -> CreatePaymentDto {
    CreatePaymentDto {
        id: Some(id.into()),
        order_id: format!("ORDER_{id}"),
        amount_cents: 105,
        currency: "usd".into(),
    }
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn live_paypal_wire_capture_claims_retries_and_signature_verification() {
    let state = Fixture::default();
    let app = Router::new()
        .route("/v1/oauth2/token", post(token))
        .route("/v2/checkout/orders", post(create))
        .route("/v2/checkout/orders/{id}/capture", post(capture))
        .route("/v2/checkout/orders/{id}", get(details))
        .route("/v1/notifications/verify-webhook-signature", post(verify))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = PaypalClient::new("owned-client".into(), "owned-secret".into(), &origin).unwrap();
    assert!(!format!("{client:?}").contains("owned-secret"));
    assert!(!format!("{client:?}").contains("owned-client"));
    let store = Arc::new(InMemoryEcommerceStore::default());
    let service = Arc::new(PaypalPaymentService::new(
        BaseEcommerceService::new(Arc::clone(&store)),
        client.clone(),
    ));
    let created = service.create_payment(request("pay")).await.unwrap();
    assert_eq!(created.status, PaymentStatus::Pending);
    assert_eq!(created.provider_ref.as_deref(), Some("ORDER_pay"));
    {
        let calls = state.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[1]["body"],
            json!({"intent":"CAPTURE","purchase_units":[{"reference_id":"ORDER_pay","amount":{"currency_code":"USD","value":"1.05"}}]})
        );
    }
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let service = Arc::clone(&service);
        jobs.push(tokio::spawn(async move {
            service
                .confirm_payment(ConfirmPaymentDto {
                    provider_ref: "ORDER_pay".into(),
                })
                .await
                .unwrap()
        }));
    }
    for job in jobs {
        assert_eq!(job.await.unwrap().status, PaymentStatus::Succeeded);
    }
    assert_eq!(state.captures.load(Ordering::SeqCst), 1);
    service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "ORDER_pay".into(),
        })
        .await
        .unwrap();
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "ORDER_pay".into(),
        })
        .await
        .unwrap();
    assert_eq!(state.captures.load(Ordering::SeqCst), 1);
    assert_eq!(
        service.get_payment("pay").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "missing".into()
            })
            .await
            .is_err()
    );
    assert_eq!(state.captures.load(Ordering::SeqCst), 1);
    service.create_payment(request("retry")).await.unwrap();
    state.fail_capture.store(true, Ordering::SeqCst);
    let error = service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "ORDER_retry".into(),
        })
        .await
        .unwrap_err();
    assert!(!format!("{error:?}").contains("private provider"));
    assert_eq!(
        service.get_payment("retry").await.unwrap().status,
        PaymentStatus::Pending
    );
    assert_eq!(state.captures.load(Ordering::SeqCst), 2);
    service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "ORDER_retry".into(),
        })
        .await
        .unwrap();
    assert_eq!(state.captures.load(Ordering::SeqCst), 3);
    service.create_payment(request("failed")).await.unwrap();
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "ORDER_failed".into(),
        })
        .await
        .unwrap();
    service
        .confirm_payment(ConfirmPaymentDto {
            provider_ref: "ORDER_failed".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        state.captures.load(Ordering::SeqCst),
        3,
        "matches TS: a failed payment cannot claim an external capture"
    );
    // Simulate a crash after the provisional TS-compatible claim. Recovery only GETs.
    let actor = forklaunch_core::domain::AuditContext::actor("operator", "capture-recovery");
    service.create_payment(request("recover")).await.unwrap();
    service
        .base()
        .claim_for_confirmation("ORDER_recover")
        .await
        .unwrap();
    let repaired = service
        .reconcile_captured_payment("recover", actor.clone())
        .await
        .unwrap();
    assert_eq!(repaired.status, PaymentStatus::Succeeded);
    assert_eq!(store.payment_reconciliation_audits().unwrap().len(), 1);
    assert_eq!(state.captures.load(Ordering::SeqCst), 3);
    // Repair a failed local write after actual provider completion as well.
    service.create_payment(request("repair")).await.unwrap();
    service
        .fail_payment(FailPaymentDto {
            provider_ref: "ORDER_repair".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        service
            .reconcile_captured_payment("repair", actor.clone())
            .await
            .unwrap()
            .status,
        PaymentStatus::Succeeded
    );
    for (pointer, replacement) in [
        ("/id", json!("substituted")),
        ("/status", json!("APPROVED")),
        ("/intent", json!("AUTHORIZE")),
        ("/purchase_units/0/reference_id", json!("other")),
        ("/purchase_units/0/amount/value", json!("2.05")),
        ("/purchase_units/0/amount/currency_code", json!("EUR")),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("PENDING"),
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("REFUNDED"),
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("PARTIALLY_REFUNDED"),
        ),
        (
            "/purchase_units/0/payments/captures/0/final_capture",
            json!(false),
        ),
        ("/purchase_units/0/payments/captures/0/id", json!("")),
        (
            "/purchase_units/0/payments/captures/0/amount/value",
            json!("1.0501"),
        ),
        (
            "/purchase_units/0/payments/captures/0/amount/value",
            json!("1e2"),
        ),
        (
            "/purchase_units/0/payments/captures/0/amount/value",
            json!("9223372036854775808"),
        ),
        (
            "/purchase_units/0/payments/captures/0/amount/currency_code",
            json!("EUR"),
        ),
        ("/purchase_units/0/payments/captures", json!([])),
        ("/purchase_units", json!([])),
    ] {
        let mut reply = completed_details("ORDER_recover");
        *reply.pointer_mut(pointer).unwrap() = replacement;
        *state.details.lock().unwrap() = Some(reply);
        let error = service
            .reconcile_captured_payment("recover", actor.clone())
            .await
            .unwrap_err();
        assert!(!error.message.contains("CAPTURE_owned"));
    }
    for mode in [
        "multiple-units",
        "multiple-captures",
        "refund",
        "authorization",
    ] {
        let mut reply = completed_details("ORDER_recover");
        match mode {
            "multiple-units" => {
                let unit = reply["purchase_units"][0].clone();
                reply["purchase_units"].as_array_mut().unwrap().push(unit);
            }
            "multiple-captures" => {
                let capture = reply["purchase_units"][0]["payments"]["captures"][0].clone();
                reply["purchase_units"][0]["payments"]["captures"]
                    .as_array_mut()
                    .unwrap()
                    .push(capture);
            }
            "refund" => {
                reply["purchase_units"][0]["payments"]["refunds"] = json!([{"id":"refund"}]);
            }
            _ => {
                reply["purchase_units"][0]["payments"]["authorizations"] =
                    json!([{"id":"authorization"}]);
            }
        }
        *state.details.lock().unwrap() = Some(reply);
        assert!(
            service
                .reconcile_captured_payment("recover", actor.clone())
                .await
                .is_err()
        );
    }
    *state.details.lock().unwrap() = None;
    assert!(
        service
            .reconcile_captured_payment(
                "recover",
                forklaunch_core::domain::AuditContext::system("missing-actor")
            )
            .await
            .is_err()
    );
    assert_eq!(store.payment_reconciliation_audits().unwrap().len(), 2);
    assert_eq!(
        state.captures.load(Ordering::SeqCst),
        3,
        "Recovery never initiates capture"
    );
    service.create_payment(request("void")).await.unwrap();
    assert!(
        service
            .base()
            .claim_for_confirmation("ORDER_void")
            .await
            .unwrap()
    );
    // Schema-valid explicit zero-entry payment history, based on PayPal Orders
    // v2 payment_collection (minItems=0). This is an owned fixture, not a live
    // merchant capture; omitted histories deliberately remain manual review.
    let voided = json!({"id":"ORDER_void", "status":"VOIDED", "intent":"CAPTURE",
        "update_time":"2026-10-10T00:00:00Z",
        "purchase_units":[{"reference_id":"ORDER_void", "amount":{"currency_code":"USD", "value":"1.05"},
        "payments":{"captures":[], "authorizations":[], "refunds":[]}}]});
    for (pointer, replacement) in [
        ("/id", json!("OTHER")),
        ("/status", json!("APPROVED")),
        ("/status", json!("CREATED")),
        ("/status", json!("COMPLETED")),
        ("/intent", json!("AUTHORIZE")),
        ("/purchase_units", json!([])),
        ("/purchase_units/0/reference_id", json!("other")),
        ("/purchase_units/0/amount/value", json!("1.051")),
        ("/purchase_units/0/amount/value", json!("2.05")),
        ("/purchase_units/0/amount/currency_code", json!("EUR")),
        ("/purchase_units/0/payments", json!(null)),
    ] {
        let mut invalid = voided.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        *state.details.lock().unwrap() = Some(invalid);
        assert!(
            service
                .reconcile_voided_payment("void", actor.clone())
                .await
                .is_err()
        );
    }
    for field in ["captures", "authorizations", "refunds"] {
        for value in [
            json!(null),
            json!({}),
            json!([{"status":"VOIDED"}]),
            json!([{"status":"COMPLETED"}]),
        ] {
            let mut invalid = voided.clone();
            invalid["purchase_units"][0]["payments"][field] = value;
            *state.details.lock().unwrap() = Some(invalid);
            assert!(
                service
                    .reconcile_voided_payment("void", actor.clone())
                    .await
                    .is_err()
            );
        }
        let mut missing = voided.clone();
        missing["purchase_units"][0]["payments"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        *state.details.lock().unwrap() = Some(missing);
        assert!(
            service
                .reconcile_voided_payment("void", actor.clone())
                .await
                .is_err()
        );
    }
    assert_eq!(
        service.base().get_payment("void").await.unwrap().status,
        PaymentStatus::Succeeded
    );
    assert!(store.payment_cancellation_audits().unwrap().is_empty());
    *state.details.lock().unwrap() = Some(voided);
    assert!(
        service
            .reconcile_voided_payment(
                "void",
                forklaunch_core::domain::AuditContext::system("no-actor")
            )
            .await
            .is_err()
    );
    let repaired = service
        .reconcile_voided_payment("void", actor.clone())
        .await
        .unwrap();
    assert_eq!(repaired.status, PaymentStatus::Failed);
    assert_eq!(repaired.provider_ref.as_deref(), Some("ORDER_void"));
    let audits = store.payment_cancellation_audits().unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        audits[0].evidence.canceled_at, None,
        "update_time is not a cancellation timestamp"
    );
    assert_eq!(audits[0].evidence.provider, "paypal");
    assert_eq!(audits[0].actor, actor);
    assert!(
        service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: "ORDER_void".into()
            })
            .await
            .is_err()
    );
    service
        .base()
        .release_confirmation_claim("ORDER_void")
        .await
        .unwrap();
    assert_eq!(
        service.base().get_payment("void").await.unwrap().status,
        PaymentStatus::Failed
    );
    *state.details.lock().unwrap() = None;
    assert!(
        service
            .reconcile_captured_payment("void", actor.clone())
            .await
            .is_err(),
        "cancellation blocks contradictory capture repair"
    );
    assert_eq!(
        state.captures.load(Ordering::SeqCst),
        3,
        "void recovery never captures"
    );
    for (mode, expected) in [
        ("success", true),
        ("failure", false),
        ("bad-shape", false),
        ("status-failure", false),
    ] {
        let params = VerifyWebhookSignature {
            transmission_id: "transmission".into(),
            transmission_time: "2026-10-10T00:00:00Z".into(),
            transmission_sig: "signature".into(),
            cert_url: "http://127.0.0.1:9/untrusted-cert".into(),
            auth_algo: "SHA256withRSA".into(),
            webhook_id: "WH-owned".into(),
            webhook_event: json!({"mode":mode}),
        };
        assert_eq!(
            client.verify_webhook_signature(&params).await.unwrap(),
            expected
        );
    }
    {
        let calls = state.calls.lock().unwrap();
        let last = calls
            .iter()
            .rev()
            .find(|c| c["operation"] == "verify")
            .unwrap();
        assert_eq!(
            last["body"]["cert_url"],
            "http://127.0.0.1:9/untrusted-cert"
        );
        assert_eq!(last["body"]["transmission_sig"], "signature");
        assert_eq!(last["body"]["webhook_id"], "WH-owned");
    }
    task.abort();
    let _ = task.await;
}
