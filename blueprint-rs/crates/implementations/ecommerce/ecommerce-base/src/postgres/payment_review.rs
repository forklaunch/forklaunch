use super::{PostgresEcommerceStore, storage_error};
use crate::{
    payment_review::{
        CancelledOrderPaymentReview, CancelledOrderPaymentReviewStore, cancelled, page, review,
        validate,
    },
    store::{RecordKind, not_found},
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{OrderStatus, PaymentDto};
use serde_json::Value;

type ReviewRow = (String, Option<sqlx::types::Json<Value>>, bool, bool, bool);

impl CancelledOrderPaymentReviewStore for PostgresEcommerceStore {
    async fn review_cancelled_order_payments(
        &self,
        order_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> BlueprintResult<CancelledOrderPaymentReview> {
        validate(order_id, after, limit)?;
        let mut tx = self.transaction().await?;
        // One SQL statement gives an MVCC snapshot of order, payments and audits.
        // Only the order status is selected: no address decryption or PII read.
        let rows: Vec<ReviewRow> = sqlx::query_as(r#"
            SELECT o.data->>'status', p.data,
                EXISTS(SELECT 1 FROM fl_ecommerce.payment_reconciliation_audit a WHERE a.tenant_id=$1 AND a.payment_id=p.id AND a.provider_ref=p.data->>'providerRef'),
                EXISTS(SELECT 1 FROM fl_ecommerce.payment_cancellation_audit a WHERE a.tenant_id=$1 AND a.payment_id=p.id AND a.provider_ref=p.data->>'providerRef'),
                EXISTS(SELECT 1 FROM (
                    SELECT provider,amount_cents,currency FROM fl_ecommerce.payment_reconciliation_audit a WHERE a.tenant_id=$1 AND a.payment_id=p.id AND a.provider_ref=p.data->>'providerRef'
                    UNION ALL
                    SELECT provider,amount_cents,currency FROM fl_ecommerce.payment_cancellation_audit a WHERE a.tenant_id=$1 AND a.payment_id=p.id AND a.provider_ref=p.data->>'providerRef'
                ) evidence HAVING count(DISTINCT provider)>1 OR bool_or(amount_cents::text IS DISTINCT FROM p.data->>'amountCents' OR lower(currency) IS DISTINCT FROM lower(p.data->>'currency')))
            FROM fl_ecommerce.records o
            LEFT JOIN fl_ecommerce.records p ON p.tenant_id=o.tenant_id AND p.kind='payment' AND p.data->>'orderId'=o.id AND ($3::text IS NULL OR p.id COLLATE "C" > $3 COLLATE "C")
            WHERE o.tenant_id=$1 AND o.kind='order' AND o.id=$2
            ORDER BY p.id COLLATE "C" LIMIT $4
        "#).bind(self.tenant.id()).bind(order_id).bind(after).bind(i64::try_from(limit + 1).map_err(storage_error)?)
            .fetch_all(&mut *tx).await.map_err(storage_error)?;
        let first = rows.first().ok_or_else(|| not_found(RecordKind::Order))?;
        let status: OrderStatus =
            serde_json::from_value(Value::String(first.0.clone())).map_err(storage_error)?;
        cancelled(status)?;
        let mut payments = Vec::new();
        for (_, value, captured, canceled, inconsistent) in rows {
            if let Some(value) = value {
                let payment: PaymentDto = serde_json::from_value(value.0).map_err(storage_error)?;
                payments.push(review(payment, captured, canceled, inconsistent));
            }
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(page(order_id, payments, limit))
    }
}
