//! Durable webhook gate semantics, shared by owned-memory fixtures and `PostgreSQL`.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::webhook::{
    WebhookEvent, WebhookEventOutcome, WebhookEventStore,
};
use std::{collections::BTreeMap, sync::Mutex};
pub(crate) fn validate(event: &WebhookEvent) -> BlueprintResult<()> {
    for value in [&event.provider, &event.provider_event_id, &event.event_type] {
        if value.is_empty()
            || value.len() > 1024
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(BlueprintError::new(
                "ecommerce_validation",
                "Webhook event identifiers must contain 1..=1024 non-control bytes",
            ));
        }
    }
    Ok(())
}
pub(crate) fn outcome(
    event: &WebhookEvent,
    event_type: &str,
    processed: bool,
) -> BlueprintResult<WebhookEventOutcome> {
    if event.event_type != event_type {
        return Err(BlueprintError::new(
            "ecommerce_event_conflict",
            "Webhook event ID was reused for a different type",
        ));
    }
    Ok(if processed {
        WebhookEventOutcome::AlreadyProcessed
    } else {
        WebhookEventOutcome::Retry
    })
}
#[derive(Default)]
pub struct InMemoryWebhookEventStore {
    events: Mutex<BTreeMap<(String, String), (String, bool)>>,
}
impl WebhookEventStore for InMemoryWebhookEventStore {
    async fn begin_processing(&self, event: WebhookEvent) -> BlueprintResult<WebhookEventOutcome> {
        validate(&event)?;
        let mut events = self.events.lock().map_err(crate::store::storage_error)?;
        let key = (event.provider.clone(), event.provider_event_id.clone());
        if let Some((event_type, processed)) = events.get(&key) {
            outcome(&event, event_type, *processed)
        } else {
            events.insert(key, (event.event_type, false));
            Ok(WebhookEventOutcome::New)
        }
    }
    async fn mark_processed(&self, provider: &str, event_id: &str) -> BlueprintResult<()> {
        let mut events = self.events.lock().map_err(crate::store::storage_error)?;
        let (_, processed) = events
            .get_mut(&(provider.to_owned(), event_id.to_owned()))
            .ok_or_else(|| {
                BlueprintError::new("ecommerce_not_found", "Webhook event was not received")
            })?;
        *processed = true;
        Ok(())
    }
}
