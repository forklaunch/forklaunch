use super::{PostgresEcommerceStore, storage_error};
use crate::payment_resume::{PaymentResumeStore, conflict};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderStatus, PaymentDto};
impl PaymentResumeStore for PostgresEcommerceStore {
    async fn validate_payment_resume(&self, expected: &PaymentDto) -> BlueprintResult<()> {
        let mut tx = self.transaction().await?;
        // One MVCC statement, selecting no address or other order PII.
        let status:Option<String>=sqlx::query_scalar("SELECT o.data->>'status' FROM fl_ecommerce.records p JOIN fl_ecommerce.records o ON o.tenant_id=p.tenant_id AND o.kind='order' AND o.id=p.data->>'orderId' WHERE p.tenant_id=$1 AND p.kind='payment' AND p.id=$2 AND p.data->>'id'=p.id AND o.data->>'id'=o.id AND p.data->>'orderId'=$3 AND p.data->>'providerRef' IS NOT DISTINCT FROM $4")
            .bind(self.tenant.id()).bind(&expected.id).bind(&expected.order_id).bind(&expected.provider_ref).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let status: OrderStatus =
            serde_json::from_value(serde_json::Value::String(status.ok_or_else(conflict)?))
                .map_err(|_| conflict())?;
        if status == OrderStatus::Cancelled {
            return Err(conflict());
        }
        tx.commit().await.map_err(storage_error)
    }
}
