//! Exact-byte authentication precedes durable receipts and billing synchronization.
use super::{BillingBackend, failure};
use crate::{
    services::StripeBillingService,
    synchronization::{BillingWebhookData, BillingWebhookEvent},
};
use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
fn reject(status: StatusCode) -> Response {
    (status, "Event verification failed").into_response()
}
pub(super) async fn stripe(
    backend: Arc<BillingBackend>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(verifier) = &backend.webhook else {
        return reject(StatusCode::BAD_REQUEST);
    };
    let Some(signature) = headers
        .get("stripe-signature")
        .and_then(|value| value.to_str().ok())
    else {
        return reject(StatusCode::BAD_REQUEST);
    };
    if !verifier.verify(&body, signature) {
        return reject(StatusCode::BAD_REQUEST);
    }
    let Ok(event) = serde_json::from_slice::<BillingWebhookEvent>(&body) else {
        return reject(StatusCode::BAD_REQUEST);
    };
    process(
        &backend,
        event,
        "provider:stripe",
        "Webhook event processed",
    )
    .await
}
pub(super) async fn platform(
    backend: Arc<BillingBackend>,
    feature: String,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if feature != "payments" {
        return (
            StatusCode::NOT_FOUND,
            format!("No handler for {feature} events"),
        )
            .into_response();
    }
    let Some(secret) = &backend.platform_key else {
        return reject(StatusCode::UNAUTHORIZED);
    };
    let Some(header) = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
    else {
        return reject(StatusCode::UNAUTHORIZED);
    };
    let Ok(auth) = HmacAuthorization::parse(header) else {
        return reject(StatusCode::UNAUTHORIZED);
    };
    if auth.key_id() != "platform"
        || !auth.verify_at(
            secret.as_bytes(),
            "POST",
            "/platform-events/payments",
            SignedBody::Bytes(&body),
            chrono::Utc::now().timestamp_millis(),
            Duration::from_secs(300),
        )
    {
        return reject(StatusCode::UNAUTHORIZED);
    }
    match backend.store.claim_platform_event_nonce(auth.nonce()).await {
        Ok(true) => (),
        Ok(false) => return reject(StatusCode::UNAUTHORIZED),
        Err(error) => return failure(&error),
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return reject(StatusCode::UNAUTHORIZED);
    };
    let (Some(id), Some(event_type), Some(_feature)) = (
        payload["id"].as_str(),
        payload["type"].as_str(),
        payload["feature"].as_str(),
    ) else {
        return reject(StatusCode::UNAUTHORIZED);
    };
    let event = BillingWebhookEvent {
        id: id.into(),
        event_type: event_type.into(),
        data: BillingWebhookData {
            object: payload["data"].clone(),
        },
    };
    process(&backend, event, "platform:payments", "ok").await
}
async fn apply(
    backend: &BillingBackend,
    event: BillingWebhookEvent,
    actor: &str,
) -> BlueprintResult<()> {
    event.validate()?;
    let client = backend.client.as_ref().ok_or_else(|| {
        BlueprintError::new("billing_configuration", "Stripe provider is not configured")
    })?;
    let store = Arc::new(
        backend
            .store
            .with_audit_context(AuditContext::actor(actor, uuid::Uuid::new_v4().to_string()))?,
    );
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::clone(&store))
            .with_party_types("user".into(), "organization".into())?
            .with_database_backup(backend.backup),
        client.clone(),
    );
    store
        .with_webhook_receipt("stripe", &event.id, &event.event_type, || async {
            service.synchronize_event(&event, "user").await
        })
        .await?;
    Ok(())
}
async fn process(
    backend: &BillingBackend,
    event: BillingWebhookEvent,
    actor: &str,
    message: &'static str,
) -> Response {
    match tokio::time::timeout(Duration::from_secs(60), apply(backend, event, actor)).await {
        Ok(Ok(())) => (StatusCode::OK, message).into_response(),
        Ok(Err(error)) => failure(&error),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Event processing timed out",
        )
            .into_response(),
    }
}
