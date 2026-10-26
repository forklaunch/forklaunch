use super::{PostgresEcommerceStore, storage_error};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::webhook::{
    WebhookEvent, WebhookEventOutcome, WebhookEventStore,
};
impl WebhookEventStore for PostgresEcommerceStore {
    async fn begin_processing(&self, event: WebhookEvent) -> BlueprintResult<WebhookEventOutcome> {
        crate::webhook::validate(&event)?;
        let mut tx = self.transaction().await?;
        let inserted=sqlx::query("INSERT INTO fl_ecommerce.webhook_events(tenant_id,provider,provider_event_id,event_type) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(self.tenant.id()).bind(&event.provider).bind(&event.provider_event_id).bind(&event.event_type).execute(&mut *tx).await.map_err(storage_error)?.rows_affected()==1;
        let result = if inserted {
            WebhookEventOutcome::New
        } else {
            let (event_type,processed):(String,bool)=sqlx::query_as("SELECT event_type,processed FROM fl_ecommerce.webhook_events WHERE tenant_id=$1 AND provider=$2 AND provider_event_id=$3")
                .bind(self.tenant.id()).bind(&event.provider).bind(&event.provider_event_id).fetch_one(&mut *tx).await.map_err(storage_error)?;
            crate::webhook::outcome(&event, &event_type, processed)?
        };
        tx.commit().await.map_err(storage_error)?;
        Ok(result)
    }
    async fn mark_processed(&self, provider: &str, event_id: &str) -> BlueprintResult<()> {
        let mut tx = self.transaction().await?;
        let updated=sqlx::query("UPDATE fl_ecommerce.webhook_events SET processed=true,updated_at=now() WHERE tenant_id=$1 AND provider=$2 AND provider_event_id=$3")
            .bind(self.tenant.id()).bind(provider).bind(event_id).execute(&mut *tx).await.map_err(storage_error)?.rows_affected();
        if updated == 0 {
            return Err(BlueprintError::new(
                "ecommerce_not_found",
                "Webhook event was not received",
            ));
        }
        tx.commit().await.map_err(storage_error)
    }
}
