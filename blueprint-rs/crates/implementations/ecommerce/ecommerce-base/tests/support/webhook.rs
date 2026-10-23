use forklaunch_ecommerce_interface::webhook::{
    WebhookEvent, WebhookEventOutcome, WebhookEventStore,
};
use std::sync::Arc;
pub async fn ledger_rules<S: WebhookEventStore + 'static>(store: Arc<S>) {
    let event = WebhookEvent {
        provider: "stripe".to_owned(),
        provider_event_id: "ledger-replay".to_owned(),
        event_type: "payment_intent.succeeded".to_owned(),
    };
    assert_eq!(
        store.begin_processing(event.clone()).await.unwrap(),
        WebhookEventOutcome::New
    );
    assert_eq!(
        store.begin_processing(event.clone()).await.unwrap(),
        WebhookEventOutcome::Retry
    );
    store
        .mark_processed(&event.provider, &event.provider_event_id)
        .await
        .unwrap();
    store
        .mark_processed(&event.provider, &event.provider_event_id)
        .await
        .unwrap();
    assert_eq!(
        store.begin_processing(event.clone()).await.unwrap(),
        WebhookEventOutcome::AlreadyProcessed
    );
    assert_eq!(
        store
            .begin_processing(WebhookEvent {
                event_type: "different".to_owned(),
                ..event.clone()
            })
            .await
            .unwrap_err()
            .code,
        "ecommerce_event_conflict"
    );
    assert_eq!(
        store
            .begin_processing(WebhookEvent {
                provider: "paypal".to_owned(),
                ..event.clone()
            })
            .await
            .unwrap(),
        WebhookEventOutcome::New
    );
    assert_eq!(
        store
            .mark_processed("stripe", "missing-event")
            .await
            .unwrap_err()
            .code,
        "ecommerce_not_found"
    );
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let store = Arc::clone(&store);
        let event = WebhookEvent {
            provider_event_id: "ledger-concurrent".to_owned(),
            ..event.clone()
        };
        tasks.push(tokio::spawn(async move {
            store.begin_processing(event).await.unwrap()
        }));
    }
    let mut fresh = 0;
    for task in tasks {
        match task.await.unwrap() {
            WebhookEventOutcome::New => fresh += 1,
            WebhookEventOutcome::Retry => {}
            WebhookEventOutcome::AlreadyProcessed => panic!("Unprocessed event became processed"),
        }
    }
    assert_eq!(fresh, 1);
}
