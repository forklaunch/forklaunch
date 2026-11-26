use axum::{Json, Router, extract::State, routing::get};
use forklaunch_core::http::provider_http::ProviderHttpClient;
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, InMemoryEcommerceStore, Mutation, Record, RecordKind, Selector},
};
use forklaunch_ecommerce_interface::{CreatePaymentDto, OrderDto, OrderStatus};
use forklaunch_ecommerce_stripe::{StripeClient, StripePaymentService};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Clone)]
struct Fixture {
    store: Arc<InMemoryEcommerceStore>,
    calls: Arc<AtomicUsize>,
    change: Arc<AtomicUsize>,
}
fn order(status: OrderStatus) -> OrderDto {
    serde_json::from_value(json!({"id":"order","items":[],"shippingAddress":{"name":"private","line1":"private","city":"x","state":"x","postalCode":"x","country":"US"},"subtotalCents":100,"discountCents":0,"taxCents":0,"taxBreakdown":[],"shippingCents":0,"giftCardCents":0,"totalCents":100,"status":status})).unwrap()
}
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn guarded_resume_denies_cancellation_before_and_during_get() {
    let store = Arc::new(InMemoryEcommerceStore::default());
    let base = BaseEcommerceService::new(store.clone());
    base.create_payment_with_reference(
        CreatePaymentDto {
            id: Some("payment".into()),
            order_id: "order".into(),
            amount_cents: 100,
            currency: "usd".into(),
        },
        Some("pi_owned".into()),
    )
    .await
    .unwrap();
    let fixture = Fixture {
        store: store.clone(),
        calls: Arc::new(AtomicUsize::new(0)),
        change: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route(
            "/v1/payment_intents/pi_owned",
            get(|State(state): State<Fixture>| async move {
                state.calls.fetch_add(1, Ordering::SeqCst);
                if state.change.load(Ordering::SeqCst) == 1 {
                    state
                        .store
                        .mutate(
                            Selector::id(RecordKind::Order, "order"),
                            Mutation::TransitionOrder {
                                expected: OrderStatus::Pending,
                                to: OrderStatus::Cancelled,
                            },
                        )
                        .await
                        .unwrap();
                }
                if state.change.load(Ordering::SeqCst) == 2 {
                    state
                        .store
                        .delete(Selector::id(RecordKind::Order, "order"))
                        .await
                        .unwrap();
                }
                Json(json!({"id":"pi_owned","client_secret":"response-only-secret"}))
            }),
        )
        .with_state(fixture.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = StripeClient::with_transport(
        "owned-secret".into(),
        ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(5), 1024)
            .unwrap(),
    )
    .unwrap();
    let service = StripePaymentService::new(base, client);
    assert!(service.resume_payment_for_order("payment").await.is_err());
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    for status in [
        OrderStatus::Pending,
        OrderStatus::Paid,
        OrderStatus::Fulfilled,
        OrderStatus::Shipped,
        OrderStatus::Delivered,
        OrderStatus::Cancelled,
    ] {
        store
            .delete(Selector::id(RecordKind::Order, "order"))
            .await
            .unwrap();
        store.insert(Record::Order(order(status))).await.unwrap();
        let before = fixture.calls.load(Ordering::SeqCst);
        let result = service.resume_payment_for_order("payment").await;
        if status == OrderStatus::Cancelled {
            assert!(result.is_err());
            assert_eq!(fixture.calls.load(Ordering::SeqCst), before);
        } else {
            assert_eq!(
                result.unwrap().client_secret.as_deref(),
                Some("response-only-secret")
            );
            assert_eq!(fixture.calls.load(Ordering::SeqCst), before + 1);
        }
    }
    for change in [1, 2] {
        store
            .delete(Selector::id(RecordKind::Order, "order"))
            .await
            .unwrap();
        store
            .insert(Record::Order(order(OrderStatus::Pending)))
            .await
            .unwrap();
        fixture.change.store(change, Ordering::SeqCst);
        assert!(service.resume_payment_for_order("payment").await.is_err());
    }
    // Explicit generic legacy path remains usable without an order guard.
    fixture.change.store(0, Ordering::SeqCst);
    assert!(
        service
            .resume_payment("payment")
            .await
            .unwrap()
            .client_secret
            .is_some()
    );
    server.abort();
}

#[tokio::test]
async fn retrieve_rejects_mismatched_or_malformed_response_identity() {
    let body = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
    let app = Router::new()
        .route(
            "/v1/payment_intents/pi_owned",
            get(
                |State(body): State<Arc<std::sync::Mutex<serde_json::Value>>>| async move {
                    Json(body.lock().unwrap().clone())
                },
            ),
        )
        .with_state(body.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = StripeClient::with_transport(
        "owned-secret".into(),
        ProviderHttpClient::new(&format!("http://{address}"), Duration::from_secs(5), 1024)
            .unwrap(),
    )
    .unwrap();
    for response in [
        json!({"id":"pi_other","client_secret":"must-not-return"}),
        json!({"client_secret":"must-not-return"}),
        json!({"id":42,"client_secret":"must-not-return"}),
        json!({"id":null,"client_secret":"must-not-return"}),
        serde_json::Value::Null,
    ] {
        *body.lock().unwrap() = response;
        let error = client.retrieve_intent("pi_owned", None).await.unwrap_err();
        assert!(!format!("{error:?}").contains("must-not-return"));
        assert!(!format!("{error:?}").contains("pi_other"));
    }
    *body.lock().unwrap() = json!({"id":"pi_owned","client_secret":"owned-secret"});
    assert_eq!(
        client
            .retrieve_intent("pi_owned", None)
            .await
            .unwrap()
            .client_secret
            .as_deref(),
        Some("owned-secret")
    );
    server.abort();
}
