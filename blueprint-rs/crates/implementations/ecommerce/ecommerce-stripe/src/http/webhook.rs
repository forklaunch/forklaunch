//! Provider-authenticated callbacks. Raw payloads are verified before any business state changes.
use super::{EcommerceBackend, failure};
use axum::{
    Extension,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_base::{BaseEcommerceService, postgres::PostgresEcommerceStore};
use forklaunch_ecommerce_interface::{
    CartService, ConfirmPaymentDto, FailPaymentDto, OrderService, OrderStatus, PaymentService,
    TransitionOrderDto,
    webhook::{PaypalWebhookEvent, WebhookEvent, WebhookEventOutcome, WebhookEventStore},
};
use forklaunch_ecommerce_paypal::{PaypalPaymentService, VerifyWebhookSignature};
use serde_json::{Value, json};
use std::sync::Arc;
fn rejected() -> Response {
    (StatusCode::BAD_REQUEST, "Webhook verification failed").into_response()
}
fn invalid() -> BlueprintError {
    BlueprintError::new("ecommerce_validation", "Invalid webhook event")
}
fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
pub(super) async fn handle_stripe_webhook(
    Extension(backend): Extension<Arc<EcommerceBackend>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    stripe(&backend, &headers, &body).await
}
/// Verifies a raw Stripe callback against the configured signing secret, then
/// records and applies it once.
pub(super) async fn stripe(
    backend: &EcommerceBackend,
    headers: &HeaderMap,
    body: &Bytes,
) -> Response {
    let Some(verifier) = &backend.stripe_webhook else {
        return rejected();
    };
    let Some(signature) = header(headers, "stripe-signature") else {
        return rejected();
    };
    if !verifier.verify(body, &signature) {
        return rejected();
    }
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return rejected();
    };
    let (Some(id), Some(event_type)) = (payload["id"].as_str(), payload["type"].as_str()) else {
        return rejected();
    };
    let event = WebhookEvent {
        provider: "stripe".to_owned(),
        provider_event_id: id.to_owned(),
        event_type: event_type.to_owned(),
    };
    process_verified(backend, event, payload).await
}
pub(super) async fn handle_paypal_webhook(
    Extension(backend): Extension<Arc<EcommerceBackend>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    paypal(&backend, &headers, &body).await
}
/// Verifies a raw `PayPal` callback through the provider's verification API,
/// then records and applies it once.
pub(super) async fn paypal(
    backend: &EcommerceBackend,
    headers: &HeaderMap,
    body: &Bytes,
) -> Response {
    let (Some(client), Some(webhook_id)) = (&backend.paypal, &backend.paypal_webhook_id) else {
        return rejected();
    };
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return rejected();
    };
    let Ok(event) = serde_json::from_value::<PaypalWebhookEvent>(payload.clone()) else {
        return rejected();
    };
    let params = (|| {
        Some(VerifyWebhookSignature {
            transmission_id: header(headers, "paypal-transmission-id")?,
            transmission_time: header(headers, "paypal-transmission-time")?,
            transmission_sig: header(headers, "paypal-transmission-sig")?,
            cert_url: header(headers, "paypal-cert-url")?,
            auth_algo: header(headers, "paypal-auth-algo")?,
            webhook_id: webhook_id.clone(),
            webhook_event: payload.clone(),
        })
    })();
    let Some(params) = params else {
        return rejected();
    };
    // Keep every field of the received JSON; typed validation must not rewrite
    // the event sent to the provider's verification API or fetch cert URLs locally.
    if !matches!(client.verify_webhook_signature(&params).await, Ok(true)) {
        return rejected();
    }
    process_verified(
        backend,
        WebhookEvent {
            provider: "paypal".to_owned(),
            provider_event_id: event.id,
            event_type: event.event_type,
        },
        payload,
    )
    .await
}
async fn process_verified(
    backend: &EcommerceBackend,
    event: WebhookEvent,
    payload: Value,
) -> Response {
    let Ok(_slot) = backend.orchestration_slots.acquire().await else {
        return failure(&invalid());
    };
    let identity = json!([event.provider, event.provider_event_id]).to_string();
    let _event_guard = match backend
        .store
        .lock_operation("webhook-event", &identity)
        .await
    {
        Ok(guard) => guard,
        Err(error) => return failure(&error),
    };
    match backend.store.begin_processing(event.clone()).await {
        Ok(WebhookEventOutcome::AlreadyProcessed) => {
            return (StatusCode::OK, "Webhook event already processed").into_response();
        }
        Ok(_) => {}
        Err(error) => return failure(&error),
    }
    let result = if event.provider == "stripe" {
        process_stripe(backend, &event.event_type, &payload).await
    } else {
        process_paypal(backend, &event.event_type, payload).await
    };
    if result.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Webhook handling failed").into_response();
    }
    if let Err(error) = backend
        .store
        .mark_processed(&event.provider, &event.provider_event_id)
        .await
    {
        return failure(&error);
    }
    (StatusCode::OK, "Webhook event processed").into_response()
}
fn provider_service(
    backend: &EcommerceBackend,
    provider: &str,
) -> BlueprintResult<BaseEcommerceService<PostgresEcommerceStore>> {
    backend.audited_service(AuditContext::actor(
        format!("provider:{provider}"),
        uuid::Uuid::new_v4().to_string(),
    ))
}
async fn process_stripe(
    backend: &EcommerceBackend,
    event_type: &str,
    payload: &Value,
) -> BlueprintResult<()> {
    if !matches!(
        event_type,
        "payment_intent.succeeded" | "payment_intent.payment_failed"
    ) {
        return Ok(());
    }
    let object = &payload["data"]["object"];
    let reference = object["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?;
    let identity = json!(["stripe", reference]).to_string();
    let _payment_guard = backend
        .store
        .lock_operation("payment-reference", &identity)
        .await?;
    let service = provider_service(backend, "stripe")?;
    if event_type == "payment_intent.payment_failed" {
        service
            .fail_payment(FailPaymentDto {
                provider_ref: reference.to_owned(),
            })
            .await?;
    } else {
        // Preserve the TS confirmation-before-reconciliation behavior, while
        // gating order state and inventory on the stored amount and currency.
        let payment = service
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: reference.to_owned(),
            })
            .await?;
        if object["amount"].as_i64() != Some(payment.amount_cents)
            || object["currency"].as_str() != Some(payment.currency.as_str())
        {
            tracing::warn!("Stripe webhook amount/currency mismatch; order left unchanged");
            return Ok(());
        }
        transition_paid(backend, &service, &payment.order_id).await?;
    }
    Ok(())
}
async fn process_paypal(
    backend: &EcommerceBackend,
    event_type: &str,
    payload: Value,
) -> BlueprintResult<()> {
    if !matches!(
        event_type,
        "CHECKOUT.ORDER.APPROVED" | "PAYMENT.CAPTURE.DENIED"
    ) {
        return Ok(());
    }
    let event: PaypalWebhookEvent = serde_json::from_value(payload).map_err(|_| invalid())?;
    let reference = if event_type == "CHECKOUT.ORDER.APPROVED" {
        event.resource.id.as_deref()
    } else {
        event.related_order_id()
    };
    let Some(reference) = reference.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let identity = json!(["paypal", reference]).to_string();
    let _payment_guard = backend
        .store
        .lock_operation("payment-reference", &identity)
        .await?;
    let service = provider_service(backend, "paypal")?;
    if event_type == "PAYMENT.CAPTURE.DENIED" {
        service
            .fail_payment(FailPaymentDto {
                provider_ref: reference.to_owned(),
            })
            .await?;
    } else {
        let client = backend.paypal.clone().ok_or_else(invalid)?;
        let payment = PaypalPaymentService::new(service.clone(), client)
            .confirm_payment(ConfirmPaymentDto {
                provider_ref: reference.to_owned(),
            })
            .await?;
        let currency = event
            .amount()
            .and_then(|amount| amount.currency_code.as_ref())
            .map(|currency| currency.to_lowercase());
        if event.amount_cents() != Some(payment.amount_cents)
            || currency.as_deref() != Some(payment.currency.as_str())
        {
            tracing::warn!(
                "PayPal webhook amount/currency missing or mismatched; order left unchanged"
            );
            return Ok(());
        }
        transition_paid(backend, &service, &payment.order_id).await?;
    }
    Ok(())
}
async fn transition_paid(
    backend: &EcommerceBackend,
    service: &BaseEcommerceService<PostgresEcommerceStore>,
    id: &str,
) -> BlueprintResult<()> {
    let before = service.get_order(id).await?;
    if before.status != OrderStatus::Pending {
        return Ok(());
    }
    let updated = service
        .store()
        .transition_order_with_event(
            TransitionOrderDto {
                id: id.to_owned(),
                to: OrderStatus::Paid,
            },
            &backend.queue,
            "order-events",
        )
        .await?;
    if let Some(cart_id) = updated.cart_id {
        // Payment/order/event already committed. A stale cart must not make the
        // provider redeliver a successfully handled payment event.
        if service.clear_cart(&cart_id).await.is_err() {
            tracing::warn!("Paid order cart clearing failed");
        }
    }
    Ok(())
}
