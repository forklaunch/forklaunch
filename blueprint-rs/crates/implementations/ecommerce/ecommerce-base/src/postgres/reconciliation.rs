use super::{PostgresEcommerceStore, storage_error};
use crate::{
    reconciliation::{PaymentReconciliationStore, VerifiedCapture, repair, validate},
    store::{Record, RecordKind, not_found},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::PaymentDto;
use serde_json::Value;
impl PaymentReconciliationStore for PostgresEcommerceStore {
    async fn record_verified_capture(
        &self,
        expected: PaymentDto,
        evidence: VerifiedCapture,
        actor: AuditContext,
    ) -> BlueprintResult<PaymentDto> {
        validate(&expected, &evidence, &actor)?;
        let mut tx = self.transaction().await?;
        let data: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(
            "SELECT data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='payment' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&expected.id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let data = data.ok_or_else(|| not_found(RecordKind::Payment))?;
        let current: PaymentDto = serde_json::from_value(data.0).map_err(storage_error)?;
        if super::cancellation::is_canceled(&mut tx, self.tenant.id(), &current).await? {
            return Err(crate::cancellation::conflict());
        }
        let payment = repair(&current, &expected)?;
        sqlx::query("INSERT INTO fl_ecommerce.payment_reconciliation_audit(id,tenant_id,payment_id,previous_status,provider,provider_ref,capture_id,amount_cents,currency,actor_id,correlation_id) VALUES($1::uuid,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(&payment.id)
            .bind(serde_json::to_value(current.status).map_err(storage_error)?.as_str().ok_or_else(||storage_error("status"))?)
            .bind(&evidence.provider).bind(&evidence.provider_ref).bind(&evidence.capture_id)
            .bind(evidence.amount_cents).bind(&evidence.currency).bind(&actor.actor_id)
            .bind(&actor.correlation_id).execute(&mut *tx).await.map_err(storage_error)?;
        if payment != current {
            let data = self.encode(&Record::Payment(payment.clone()))?;
            sqlx::query("UPDATE fl_ecommerce.records SET data=$3 WHERE tenant_id=$1 AND kind='payment' AND id=$2")
                .bind(self.tenant.id()).bind(&payment.id).bind(sqlx::types::Json(data))
                .execute(&mut *tx).await.map_err(storage_error)?;
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(payment)
    }
}
