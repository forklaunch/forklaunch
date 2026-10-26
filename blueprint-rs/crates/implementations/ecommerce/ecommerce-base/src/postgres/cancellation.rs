use super::{PostgresEcommerceStore, storage_error};
use crate::{
    cancellation::{PaymentCancellationStore, VerifiedCancellation, conflict, repair, validate},
    store::{Record, RecordKind, not_found},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::PaymentDto;
use serde_json::Value;

pub(super) async fn is_canceled(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    payment: &PaymentDto,
) -> BlueprintResult<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_ecommerce.payment_cancellation_audit WHERE tenant_id=$1 AND payment_id=$2 AND provider_ref=$3)")
        .bind(tenant).bind(&payment.id).bind(&payment.provider_ref).fetch_one(&mut **tx).await.map_err(storage_error)
}
impl PaymentCancellationStore for PostgresEcommerceStore {
    async fn record_verified_cancellation(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCancellation,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        validate(&expected, &evidence, &actor)?;
        let mut tx = self.transaction().await?;
        let data: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&expected.id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let data = data.ok_or_else(|| not_found(RecordKind::Payment))?;
        let current: PaymentDto = serde_json::from_value(data.0).map_err(storage_error)?;
        let captured: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_ecommerce.payment_reconciliation_audit WHERE tenant_id=$1 AND payment_id=$2 AND provider_ref=$3)")
            .bind(self.tenant.id()).bind(&expected.id).bind(&expected.provider_ref).fetch_one(&mut *tx).await.map_err(storage_error)?;
        if captured {
            return Err(conflict());
        }
        let payment = repair(&current, &expected)?;
        sqlx::query("INSERT INTO fl_ecommerce.payment_cancellation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,canceled_at,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(&payment.id)
            .bind(serde_json::to_value(current.status).map_err(storage_error)?.as_str().ok_or_else(||storage_error("status"))?)
            .bind(&evidence.provider).bind(&evidence.provider_ref).bind(evidence.canceled_at)
            .bind(evidence.amount_cents).bind(&evidence.currency).bind(&actor.actor_id).bind(&actor.correlation_id)
            .execute(&mut *tx).await.map_err(storage_error)?;
        if payment != current {
            let data = self.encode(&Record::Payment(payment.clone()))?;
            sqlx::query("UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2")
                .bind(self.tenant.id()).bind(&payment.id).bind(sqlx::types::Json(data)).execute(&mut *tx).await.map_err(storage_error)?;
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(payment)
    }
}
