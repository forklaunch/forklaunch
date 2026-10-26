//! Durable intent and explicit resolution for ambiguous webhook effects.
use super::{PostgresBillingStore, db_error, invalid, validate_id};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWebhook {
    pub provider: String,
    pub event_id: String,
    pub event_type: String,
    pub attempt_id: String,
    pub started_at: String,
}

/// Operator assertion after independently checking provider and local effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookResolution {
    /// All effects have been completed or repaired; suppress future deliveries.
    Applied,
    /// No effects remain that make re-execution unsafe; permit a fresh attempt.
    RetryAllowed,
}

impl PostgresBillingStore {
    /// Commits intent before executing effects and records success afterwards.
    /// Errors, cancellation and crashes leave an attempt requiring reconciliation.
    /// The callback's external/local writes are not one atomic transaction.
    /// Dedicate a pool with at least two connections; clones share admission limits.
    /// # Errors
    /// Rejects malformed IDs, conflicting types, busy or unresolved deliveries,
    /// database failures and callback errors. Never automatically repeats effects.
    pub async fn with_webhook_receipt<F, Fut>(
        &self,
        provider: &str,
        event_id: &str,
        event_type: &str,
        apply: F,
    ) -> BlueprintResult<bool>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = BlueprintResult<()>> + Send,
    {
        if self.pool.options().get_max_connections() < 2 {
            return Err(BlueprintError::new(
                "billing_configuration",
                "Webhook processing requires at least two database connections",
            ));
        }
        let _permit = self.webhook_slots.try_acquire().map_err(|_| busy())?;
        for value in [provider, event_id, event_type] {
            validate_id(value)?;
        }
        let mut tx = self.transaction().await?;
        self.lock_webhook(&mut tx, provider, event_id).await?;
        let recorded: Option<String> = sqlx::query_scalar(
            "SELECT event_type FROM fl_billing.webhook_receipts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3")
            .bind(self.tenant.id()).bind(provider).bind(event_id)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        if let Some(recorded) = recorded {
            check_type(&recorded, event_type)?;
            tx.commit().await.map_err(db_error)?;
            return Ok(false);
        }
        let pending: Option<String> = sqlx::query_scalar(
            "SELECT event_type FROM fl_billing.webhook_attempts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3")
            .bind(self.tenant.id()).bind(provider).bind(event_id)
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        if let Some(pending) = pending {
            check_type(&pending, event_type)?;
            return Err(BlueprintError::new(
                "billing_webhook_reconciliation_required",
                "Billing event requires reconciliation before replay",
            ));
        }
        // Keep the event lock while committing intent on a separate connection.
        // A crash at any later point leaves durable evidence, including commit ambiguity.
        let mut intent = self.transaction().await?;
        sqlx::query("INSERT INTO fl_billing.webhook_attempts(tenant_id,provider,event_id,event_type,attempt_id) VALUES($1,$2,$3,$4,$5::uuid)")
            .bind(self.tenant.id()).bind(provider).bind(event_id).bind(event_type)
            .bind(uuid::Uuid::new_v4().to_string()).execute(&mut *intent).await.map_err(db_error)?;
        intent.commit().await.map_err(db_error)?;
        if let Err(error) = apply().await {
            // Release the event lock before returning an ordinary callback
            // failure. Drop queues rollback asynchronously and can otherwise
            // make immediate operator inspection spuriously report busy.
            // The separately committed intent remains pending.
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        self.complete_webhook(&mut tx, provider, event_id, event_type)
            .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(true)
    }

    /// Lists bounded outstanding attempts, including currently running callbacks.
    /// Resolving an active callback fails promptly rather than stealing its lock.
    /// # Errors
    /// Rejects limits outside 1..=1000 and sanitized storage errors.
    pub async fn pending_webhooks(&self, limit: u32) -> BlueprintResult<Vec<PendingWebhook>> {
        if !(1..=1000).contains(&limit) {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        let rows: Vec<(String,String,String,String,String)> = sqlx::query_as(
            "SELECT provider,event_id,event_type,attempt_id::text,started_at::text FROM fl_billing.webhook_attempts WHERE tenant_id=$1 ORDER BY started_at,provider,event_id LIMIT $2")
            .bind(self.tenant.id()).bind(i64::from(limit)).fetch_all(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(provider, event_id, event_type, attempt_id, started_at)| PendingWebhook {
                    provider,
                    event_id,
                    event_type,
                    attempt_id,
                    started_at,
                },
            )
            .collect())
    }

    /// Resolves exactly the inspected attempt with an atomic actor audit.
    /// This API does not verify or repair provider effects. The caller must do so
    /// before asserting `Applied` or `RetryAllowed`. A stale attempt cannot resolve a retry.
    /// # Errors
    /// Requires an actor/correlation context and a matching inactive pending attempt.
    pub async fn resolve_webhook(
        &self,
        pending: &PendingWebhook,
        resolution: WebhookResolution,
    ) -> BlueprintResult<()> {
        let actor = self.audit.actor_id.as_deref().ok_or_else(invalid)?;
        for value in [
            actor,
            self.audit.correlation_id.as_str(),
            pending.provider.as_str(),
            pending.event_id.as_str(),
            pending.event_type.as_str(),
        ] {
            validate_id(value)?;
        }
        uuid::Uuid::parse_str(&pending.attempt_id).map_err(|_| invalid())?;
        let mut tx = self.transaction().await?;
        self.lock_webhook(&mut tx, &pending.provider, &pending.event_id)
            .await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fl_billing.webhook_attempts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3 AND event_type=$4 AND attempt_id=$5::uuid)")
            .bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id)
            .bind(&pending.event_type).bind(&pending.attempt_id)
            .fetch_one(&mut *tx).await.map_err(db_error)?;
        if !exists {
            return Err(BlueprintError::new(
                "billing_webhook_resolution_conflict",
                "Pending billing attempt no longer matches",
            ));
        }
        sqlx::query("INSERT INTO fl_billing.webhook_reconciliation_audit(id,tenant_id,provider,event_id,event_type,attempt_id,resolution,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,$5,$6::uuid,$7,$8,$9)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id())
            .bind(&pending.provider).bind(&pending.event_id).bind(&pending.event_type)
            .bind(&pending.attempt_id).bind(match resolution { WebhookResolution::Applied => "applied", WebhookResolution::RetryAllowed => "retry_allowed" })
            .bind(actor).bind(&self.audit.correlation_id).execute(&mut *tx).await.map_err(db_error)?;
        match resolution {
            WebhookResolution::Applied => {
                self.complete_webhook(
                    &mut tx,
                    &pending.provider,
                    &pending.event_id,
                    &pending.event_type,
                )
                .await?;
            }
            WebhookResolution::RetryAllowed => {
                self.remove_webhook_attempt(&mut tx, &pending.provider, &pending.event_id)
                    .await?;
            }
        }
        tx.commit().await.map_err(db_error)
    }

    pub(super) async fn lock_webhook(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        provider: &str,
        event_id: &str,
    ) -> BlueprintResult<()> {
        let key = serde_json::json!([self.tenant.id(), provider, event_id]).to_string();
        let locked: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
                .bind(key)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_error)?;
        if locked { Ok(()) } else { Err(busy()) }
    }
    pub(super) async fn complete_webhook(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        provider: &str,
        event_id: &str,
        event_type: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.webhook_receipts(tenant_id,provider,event_id,event_type) VALUES($1,$2,$3,$4)")
            .bind(self.tenant.id()).bind(provider).bind(event_id).bind(event_type)
            .execute(&mut **tx).await.map_err(db_error)?;
        self.remove_webhook_attempt(tx, provider, event_id).await
    }
    async fn remove_webhook_attempt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        provider: &str,
        event_id: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("DELETE FROM fl_billing.webhook_attempts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3")
            .bind(self.tenant.id()).bind(provider).bind(event_id)
            .execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
}
fn check_type(actual: &str, expected: &str) -> BlueprintResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(BlueprintError::new(
            "billing_event_conflict",
            "Billing event ID was reused for another type",
        ))
    }
}
fn busy() -> BlueprintError {
    BlueprintError::new("billing_webhook_busy", "Billing event is being processed")
}
