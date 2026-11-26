//! Trusted local bookkeeping for an already-satisfied paused subscription effect.
use super::super::{PendingWebhook, db_error, invalid};
use super::{DurableStripeBinding, PostgresBillingStore, conflict};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use sqlx::{Postgres, Transaction};

type PausedRow = (sqlx::types::Json<Value>, String, String);

/// Opaque tenant/attempt/binding/row snapshot; no public constructor or serialization.
pub struct PausedSubscriptionEffectSnapshot {
    tenant: String,
    pending: PendingWebhook,
    binding: DurableStripeBinding,
    row: PausedRow,
}

/// Only delivery bookkeeping changed. Neither variant proves causal application
/// of the old callback or an uninterrupted provider pause epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PausedEffectAcknowledgement {
    Recorded,
    /// COMMIT succeeded, but the caller's evidence check failed afterwards.
    /// The receipt exists; do not claim rollback or repeat provider effects.
    RecordedEvidenceNoLongerCurrent,
}

fn target(pending: &PendingWebhook) -> BlueprintResult<()> {
    if pending.provider != "stripe" || pending.event_type != "customer.subscription.paused" {
        return Err(conflict());
    }
    super::super::validate_id(&pending.event_id)?;
    uuid::Uuid::parse_str(&pending.attempt_id).map_err(|_| invalid())?;
    Ok(())
}
impl PostgresBillingStore {
    async fn lock_paused_attempt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pending: &PendingWebhook,
    ) -> BlueprintResult<()> {
        target(pending)?;
        self.lock_webhook(tx, &pending.provider, &pending.event_id)
            .await?;
        let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_billing.webhook_attempts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3 AND event_type=$4 AND attempt_id=$5::uuid AND started_at::text=$6)")
            .bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id).bind(&pending.event_type).bind(&pending.attempt_id).bind(&pending.started_at).fetch_one(&mut **tx).await.map_err(db_error)?;
        if !matches {
            return Err(conflict());
        }
        Ok(())
    }
    async fn paused_bound_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeBinding,
    ) -> BlueprintResult<PausedRow> {
        if binding.tenant != self.tenant.id() || binding.kind != BillingKind::Subscription {
            return Err(conflict());
        }
        let relation = binding
            .subscription_ownership
            .as_ref()
            .ok_or_else(conflict)?;
        sqlx::query_as("SELECT r.data,r.xmin::text,b.xmin::text FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id WHERE b.tenant_id=$1 AND b.kind='subscription' AND b.public_id=$2::uuid AND b.binding_id=$3::uuid AND b.storage_id=$4 AND b.provider_id=$5 AND b.deployment_label=$6 AND b.account_id=$7 AND b.livemode=$8 AND b.subscription_provider_customer_id=$9 AND b.local_party_id=$10 AND b.local_party_type=$11 FOR UPDATE OF b,r")
            .bind(self.tenant.id()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string()).bind(&binding.storage_id).bind(&binding.provider_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode)
            .bind(&relation.provider_customer_id).bind(&relation.local_party_id).bind(&relation.local_party_type).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(conflict)
    }
    /// Trusted server inspection only: provider evidence must be authenticated
    /// separately. Requires existing exact local ownership and active=false.
    /// # Errors
    /// Rejects active/stale attempts, missing actors, ownership and audit failures.
    pub async fn inspect_paused_subscription_effect(
        &self,
        pending: &PendingWebhook,
        binding: &DurableStripeBinding,
    ) -> BlueprintResult<PausedSubscriptionEffectSnapshot> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        self.lock_paused_attempt(&mut tx, pending).await?;
        let row = self.paused_bound_row(&mut tx, binding).await?;
        let value = self
            .decode(
                &mut tx,
                Scope::Durable,
                BillingKind::Subscription,
                &binding.storage_id,
                row.0.0.clone(),
                "paused_effect_inspect",
            )
            .await?;
        if value["active"].as_bool() != Some(false)
            || !binding
                .subscription_ownership
                .as_ref()
                .ok_or_else(conflict)?
                .matches(&value, &binding.provider_id)
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(PausedSubscriptionEffectSnapshot {
            tenant: self.tenant.id().into(),
            pending: pending.clone(),
            binding: binding.clone(),
            row,
        })
    }
    /// Acknowledges an already-satisfied effect, never changes a subscription.
    /// `evidence_current` must verify the same authenticated provider observation
    /// and client generation, including its deadline. Called after awaited locks,
    /// before COMMIT, and after COMMIT. No durable COMMIT-instant freshness claim.
    /// Cancellation/transport failure during COMMIT can have an ambiguous result;
    /// query `paused_subscription_effect_receipt_recorded` before further action.
    /// # Errors
    /// Rejects stale evidence/snapshots/attempts and atomic audit/receipt failures.
    pub async fn acknowledge_paused_subscription_effect<F>(
        &self,
        snapshot: PausedSubscriptionEffectSnapshot,
        evidence_current: F,
    ) -> BlueprintResult<PausedEffectAcknowledgement>
    where
        F: Fn() -> BlueprintResult<()> + Send,
    {
        self.binding_actor()?;
        if snapshot.tenant != self.tenant.id() {
            return Err(conflict());
        }
        evidence_current()?;
        let mut tx = self.transaction().await?;
        self.lock_paused_attempt(&mut tx, &snapshot.pending).await?;
        evidence_current()?;
        if self.paused_bound_row(&mut tx, &snapshot.binding).await? != snapshot.row {
            return Err(conflict());
        }
        evidence_current()?;
        let pending = &snapshot.pending;
        sqlx::query("INSERT INTO fl_billing.webhook_reconciliation_audit(id,tenant_id,provider,event_id,event_type,attempt_id,resolution,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,$5,$6::uuid,'applied',$7,$8)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id).bind(&pending.event_type).bind(&pending.attempt_id).bind(self.binding_actor()?).bind(&self.audit.correlation_id).execute(&mut *tx).await.map_err(db_error)?;
        self.complete_webhook(
            &mut tx,
            &pending.provider,
            &pending.event_id,
            &pending.event_type,
        )
        .await?;
        evidence_current()?;
        tx.commit().await.map_err(db_error)?;
        Ok(if evidence_current().is_ok() {
            PausedEffectAcknowledgement::Recorded
        } else {
            PausedEffectAcknowledgement::RecordedEvidenceNoLongerCurrent
        })
    }
    /// Read-only receipt query after timeout/cancellation or late evidence expiry.
    /// # Errors
    /// Rejects absent actors, unsupported event types and conflicting receipts.
    pub async fn paused_subscription_effect_receipt_recorded(
        &self,
        pending: &PendingWebhook,
    ) -> BlueprintResult<bool> {
        self.binding_actor()?;
        target(pending)?;
        let mut tx = self.transaction().await?;
        let event_type:Option<String>=sqlx::query_scalar("SELECT event_type FROM fl_billing.webhook_receipts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3")
            .bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        if event_type
            .as_ref()
            .is_some_and(|kind| kind != &pending.event_type)
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(event_type.is_some())
    }
}
