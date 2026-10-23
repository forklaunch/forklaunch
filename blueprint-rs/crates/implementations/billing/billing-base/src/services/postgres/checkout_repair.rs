//! Trusted local completion after authenticated, already-applied provider evidence.
use super::{PendingWebhook, PostgresBillingStore, db_error, invalid, validate_id};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use sqlx::{Postgres, Transaction};

type CheckoutRow = (String, sqlx::types::Json<Value>, String);

/// Opaque tenant/attempt/local-row snapshot, including row versions. Contains
/// encrypted provider fields; deliberately has no `Debug`/`Serialize` or public constructor.
pub struct CheckoutRepairSnapshot {
    tenant: String,
    pending: PendingWebhook,
    session_id: String,
    backup: bool,
    rows: Vec<CheckoutRow>,
}
fn conflict() -> BlueprintError {
    BlueprintError::new(
        "billing_checkout_repair_conflict",
        "Checkout repair evidence or local state no longer matches",
    )
}
fn status(pending: &PendingWebhook) -> BlueprintResult<&'static str> {
    if pending.provider != "stripe" {
        return Err(conflict());
    }
    match pending.event_type.as_str() {
        "checkout.session.completed" => Ok("SUCCESS"),
        "checkout.session.expired" => Ok("FAILED"),
        _ => Err(conflict()),
    }
}
impl PostgresBillingStore {
    async fn require_checkout_attempt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pending: &PendingWebhook,
    ) -> BlueprintResult<()> {
        status(pending)?;
        for id in [&pending.event_id, &pending.event_type] {
            validate_id(id)?;
        }
        uuid::Uuid::parse_str(&pending.attempt_id).map_err(|_| invalid())?;
        self.lock_webhook(tx, &pending.provider, &pending.event_id)
            .await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_billing.webhook_attempts WHERE tenant_id=$1 AND provider=$2 AND event_id=$3 AND event_type=$4 AND attempt_id=$5::uuid)")
            .bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id).bind(&pending.event_type).bind(&pending.attempt_id)
            .fetch_one(&mut **tx).await.map_err(db_error)?;
        if !exists {
            return Err(conflict());
        }
        Ok(())
    }
    async fn checkout_rows(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session_id: &str,
    ) -> BlueprintResult<Vec<CheckoutRow>> {
        // Include expired-but-not-purged cache rows: these still bind ownership.
        sqlx::query_as("SELECT scope,data,xmin::text FROM fl_billing.records WHERE tenant_id=$1 AND kind='checkout_session' AND id=$2 ORDER BY scope FOR UPDATE")
            .bind(self.tenant.id()).bind(session_id).fetch_all(&mut **tx).await.map_err(db_error)
    }
    /// Trusted server API: call ONLY after authenticating the pending provider event
    /// and current session terminal status plus already-applied metadata. This local
    /// method does not fetch or verify provider state. Missing local ownership stays
    /// unresolved, even when cache expiration explains its absence.
    /// # Errors
    /// Rejects active/stale attempts, missing or conflicting ownership, missing backup,
    /// contradictory terminal state, absent actor, and audited decryption failures.
    pub async fn snapshot_verified_checkout_repair(
        &self,
        pending: &PendingWebhook,
        session_id: &str,
        customer_id: &str,
        backup: bool,
    ) -> BlueprintResult<CheckoutRepairSnapshot> {
        validate_id(session_id)?;
        validate_id(customer_id)?;
        validate_id(self.audit.actor_id.as_deref().ok_or_else(invalid)?)?;
        let target = status(pending)?;
        let mut tx = self.transaction().await?;
        self.require_checkout_attempt(&mut tx, pending).await?;
        let rows = self.checkout_rows(&mut tx, session_id).await?;
        if rows.is_empty() || backup != rows.iter().any(|(scope, _, _)| scope == "durable") {
            return Err(conflict());
        }
        for (scope, raw, _) in &rows {
            let scope = match scope.as_str() {
                "cache" => Scope::Cache,
                "durable" => Scope::Durable,
                _ => return Err(conflict()),
            };
            let data = self
                .decode(
                    &mut tx,
                    scope,
                    BillingKind::CheckoutSession,
                    session_id,
                    raw.0.clone(),
                    "checkout_repair_inspect",
                )
                .await?;
            let provider = &data["providerFields"];
            let customer = provider["customer"]
                .as_str()
                .or_else(|| provider["customer"]["id"].as_str());
            if data["customerId"] != customer_id
                || provider["id"] != session_id
                || provider["object"] != "checkout.session"
                || customer != Some(customer_id)
                || (data["status"] != "PENDING" && data["status"] != target)
            {
                return Err(conflict());
            }
        }
        tx.commit().await.map_err(db_error)?;
        Ok(CheckoutRepairSnapshot {
            tenant: self.tenant.id().into(),
            pending: pending.clone(),
            session_id: session_id.into(),
            backup,
            rows,
        })
    }
    /// Atomically completes only the inspected local state and pending attempt.
    /// No provider mutation occurs; the snapshot's caller must have verified provider
    /// evidence as documented on `snapshot_verified_checkout_repair`.
    /// # Errors
    /// Rejects stale snapshots/attempts, active delivery, tenant mismatch and audit failures.
    pub async fn complete_verified_checkout_repair(
        &self,
        snapshot: CheckoutRepairSnapshot,
    ) -> BlueprintResult<()> {
        let actor = self.audit.actor_id.as_deref().ok_or_else(invalid)?;
        validate_id(actor)?;
        if snapshot.tenant != self.tenant.id() {
            return Err(conflict());
        }
        let target = status(&snapshot.pending)?;
        let mut tx = self.transaction().await?;
        self.require_checkout_attempt(&mut tx, &snapshot.pending)
            .await?;
        if self.checkout_rows(&mut tx, &snapshot.session_id).await? != snapshot.rows {
            return Err(conflict());
        }
        if snapshot.backup {
            sqlx::query("UPDATE fl_billing.records SET data=data || jsonb_build_object('status',$3::text,'updatedAt',$4::text) WHERE tenant_id=$1 AND scope='durable' AND kind='checkout_session' AND id=$2")
                .bind(self.tenant.id()).bind(&snapshot.session_id).bind(target).bind(crate::services::timestamp())
                .execute(&mut *tx).await.map_err(db_error)?;
        }
        if snapshot.rows.iter().any(|(scope, _, _)| scope == "cache") {
            sqlx::query("DELETE FROM fl_billing.records WHERE tenant_id=$1 AND scope='cache' AND kind='checkout_session' AND id=$2")
                .bind(self.tenant.id()).bind(&snapshot.session_id).execute(&mut *tx).await.map_err(db_error)?;
        }
        let pending = &snapshot.pending;
        sqlx::query("INSERT INTO fl_billing.webhook_reconciliation_audit(id,tenant_id,provider,event_id,event_type,attempt_id,resolution,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,$5,$6::uuid,'applied',$7,$8)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(&pending.provider).bind(&pending.event_id).bind(&pending.event_type).bind(&pending.attempt_id)
            .bind(actor).bind(&self.audit.correlation_id).execute(&mut *tx).await.map_err(db_error)?;
        self.complete_webhook(
            &mut tx,
            &pending.provider,
            &pending.event_id,
            &pending.event_type,
        )
        .await?;
        tx.commit().await.map_err(db_error)
    }
}
