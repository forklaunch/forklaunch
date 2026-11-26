use forklaunch_core::domain::AuditContext;
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    cancellation::{PaymentCancellationStore, VerifiedCancellation},
    payment_review::{CancelledOrderPaymentReviewStore, PaymentReviewState},
    reconciliation::{PaymentReconciliationStore, VerifiedCapture},
    store::{Record, RecordKind, Selector},
};
use forklaunch_ecommerce_interface::{OrderDto, PaymentDto, PaymentStatus};
use serde_json::json;
use std::sync::Arc;

#[allow(clippy::too_many_lines)]
pub async fn exercise<
    S: PaymentReconciliationStore + PaymentCancellationStore + CancelledOrderPaymentReviewStore,
>(
    store: Arc<S>,
) {
    let service = BaseEcommerceService::new(store.clone());
    let order: OrderDto = serde_json::from_value(json!({
        "id":"review-order", "customerId":"private-customer", "items":[],
        "shippingAddress":{"name":"private-name","line1":"private-address","city":"x","state":"x","postalCode":"x","country":"US"},
        "subtotalCents":105,"discountCents":0,"taxCents":0,"taxBreakdown":[],"shippingCents":0,"giftCardCents":0,"totalCents":105,"status":"cancelled"
    })).unwrap();
    store.insert(Record::Order(order.clone())).await.unwrap();
    let mut live = order.clone();
    live.id = "live-review-order".into();
    live.status = forklaunch_ecommerce_interface::OrderStatus::Pending;
    store.insert(Record::Order(live)).await.unwrap();
    for id in ["missing-review-order", "live-review-order"] {
        assert!(
            service
                .review_cancelled_order_payments(id, None, 5)
                .await
                .is_err()
        );
    }
    assert!(
        service
            .review_cancelled_order_payments("review-order", None, 0)
            .await
            .is_err()
    );
    assert!(
        service
            .review_cancelled_order_payments("review-order", None, 101)
            .await
            .is_err()
    );
    assert!(
        service
            .review_cancelled_order_payments("review-order", Some(""), 1)
            .await
            .is_err()
    );
    assert!(
        service
            .review_cancelled_order_payments("review-order", None, 1)
            .await
            .unwrap()
            .payments
            .is_empty()
    );
    for (id, status, reference) in [
        ("r0", PaymentStatus::Pending, None),
        ("r1", PaymentStatus::Pending, Some("r1-ref")),
        ("r2", PaymentStatus::Succeeded, Some("r2-ref")),
        ("r3", PaymentStatus::Failed, Some("r3-ref")),
        ("r4", PaymentStatus::Pending, Some("r4-ref")),
        ("r5", PaymentStatus::Pending, Some("r5-ref")),
        ("r6", PaymentStatus::Pending, Some("r6-ref")),
    ] {
        let payment = PaymentDto {
            id: id.into(),
            order_id: order.id.clone(),
            amount_cents: 105,
            currency: "usd".into(),
            status,
            provider_ref: reference.map(str::to_owned),
            created_at: None,
            updated_at: None,
        };
        store
            .insert(Record::Payment(payment.clone()))
            .await
            .unwrap();
        if id == "r4" || id == "r6" {
            store
                .record_verified_capture(
                    payment,
                    VerifiedCapture {
                        provider: "paypal".into(),
                        provider_ref: reference.unwrap().into(),
                        capture_id: format!("capture-{id}"),
                        amount_cents: 105,
                        currency: "USD".into(),
                    },
                    AuditContext::actor("operator", "review"),
                )
                .await
                .unwrap();
        } else if id == "r5" {
            store
                .record_verified_cancellation(
                    payment,
                    VerifiedCancellation {
                        provider: "paypal".into(),
                        provider_ref: reference.unwrap().into(),
                        canceled_at: None,
                        amount_cents: 105,
                        currency: "USD".into(),
                    },
                    AuditContext::actor("operator", "review"),
                )
                .await
                .unwrap();
        }
    }
    let first = service
        .review_cancelled_order_payments("review-order", None, 3)
        .await
        .unwrap();
    assert_eq!(first.next_cursor.as_deref(), Some("r2"));
    assert_eq!(
        first.payments.iter().map(|p| p.state).collect::<Vec<_>>(),
        vec![
            PaymentReviewState::NoProviderReference,
            PaymentReviewState::UnverifiedPending,
            PaymentReviewState::UnverifiedClaimOrCapture
        ]
    );
    let second = service
        .review_cancelled_order_payments("review-order", first.next_cursor.as_deref(), 4)
        .await
        .unwrap();
    assert_eq!(second.next_cursor, None);
    assert_eq!(
        second.payments.iter().map(|p| p.state).collect::<Vec<_>>(),
        vec![
            PaymentReviewState::UnverifiedFailure,
            PaymentReviewState::CapturedNeedsOperatorReview,
            PaymentReviewState::CancellationRecordedKeepReference,
            PaymentReviewState::CapturedNeedsOperatorReview
        ]
    );
    let encoded = serde_json::to_string(&second).unwrap();
    for private in [
        "private-customer",
        "private-name",
        "private-address",
        "shippingAddress",
        "actorId",
    ] {
        assert!(!encoded.contains(private));
    }
    assert!(
        service
            .review_cancelled_order_payments("review-order", Some("zz"), 3)
            .await
            .unwrap()
            .payments
            .is_empty()
    );
    // A stale historical audit must not be attached to a replacement reference.
    let replacement = PaymentDto {
        id: "r6".into(),
        order_id: order.id.clone(),
        amount_cents: 105,
        currency: "usd".into(),
        status: PaymentStatus::Pending,
        provider_ref: Some("replacement-ref".into()),
        created_at: None,
        updated_at: None,
    };
    store
        .delete(Selector::id(RecordKind::Payment, "r6"))
        .await
        .unwrap();
    store
        .insert(Record::Payment(replacement.clone()))
        .await
        .unwrap();
    let report = service
        .review_cancelled_order_payments("review-order", Some("r5"), 1)
        .await
        .unwrap();
    assert_eq!(
        report.payments[0].state,
        PaymentReviewState::UnverifiedPending
    );
    // A reused identity/reference with changed amount is visibly contradictory.
    store
        .delete(Selector::id(RecordKind::Payment, "r6"))
        .await
        .unwrap();
    store
        .insert(Record::Payment(PaymentDto {
            provider_ref: Some("r6-ref".into()),
            amount_cents: 106,
            ..replacement
        }))
        .await
        .unwrap();
    let report = service
        .review_cancelled_order_payments("review-order", Some("r5"), 1)
        .await
        .unwrap();
    assert_eq!(
        report.payments[0].state,
        PaymentReviewState::ContradictoryEvidence
    );
}
