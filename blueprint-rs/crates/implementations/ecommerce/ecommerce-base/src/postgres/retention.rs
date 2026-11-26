use super::{PostgresEcommerceStore, RecordKind, Value};
use crate::retention::{storage_error, validate};
use forklaunch_core::{
    common::FrameworkResult,
    persistence::retention::{RetentionAction, RetentionRequest, RetentionStore},
};

fn stamp(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
impl RetentionStore for PostgresEcommerceStore {
    async fn count(&self, request: RetentionRequest<'_>) -> FrameworkResult<u64> {
        validate(request, Some(&self.tenant))?;
        let mut tx = self.transaction().await.map_err(storage_error)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fl_ecommerce.records r LEFT JOIN fl_ecommerce.order_retention m ON m.tenant_id=r.tenant_id AND m.order_id=r.id WHERE r.tenant_id=$1 AND r.kind='order' AND (r.data->>'createdAt')::timestamptz < $2::text::timestamptz AND ($3 OR m.order_id IS NULL)")
            .bind(self.tenant.id()).bind(stamp(request.policy.cutoff())).bind(request.policy.action() == RetentionAction::Delete)
            .fetch_one(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        u64::try_from(count).map_err(storage_error)
    }
    async fn apply_batch(&self, request: RetentionRequest<'_>) -> FrameworkResult<usize> {
        validate(request, Some(&self.tenant))?;
        // Serialize competing retention passes; selected order row locks also
        // fence concurrent ordinary mutation or compliance erasure.
        let _lease = self
            .lock_operation("retention", "Order")
            .await
            .map_err(storage_error)?;
        let mut tx = self.transaction().await.map_err(storage_error)?;
        let rows: Vec<(String, sqlx::types::Json<Value>)> = sqlx::query_as("SELECT r.id,r.data FROM fl_ecommerce.records r LEFT JOIN fl_ecommerce.order_retention m ON m.tenant_id=r.tenant_id AND m.order_id=r.id WHERE r.tenant_id=$1 AND r.kind='order' AND (r.data->>'createdAt')::timestamptz < $2::text::timestamptz AND ($3 OR m.order_id IS NULL) ORDER BY (r.data->>'createdAt')::timestamptz,r.id LIMIT $4 FOR UPDATE OF r")
            .bind(self.tenant.id()).bind(stamp(request.policy.cutoff())).bind(request.policy.action() == RetentionAction::Delete)
            .bind(i64::try_from(request.batch_size).map_err(storage_error)?).fetch_all(&mut *tx).await.map_err(storage_error)?;
        let count = rows.len();
        for (id, data) in rows {
            // Hydration and access auditing must succeed before a mutation commits.
            self.decode(&mut tx, RecordKind::Order, &id, data.0, "retention")
                .await
                .map_err(storage_error)?;
            match request.policy.action() {
                RetentionAction::Delete => {
                    sqlx::query("DELETE FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id=$2")
                        .bind(self.tenant.id()).bind(&id).execute(&mut *tx).await.map_err(storage_error)?;
                }
                RetentionAction::Anonymize => {
                    sqlx::query("INSERT INTO fl_ecommerce.order_retention(tenant_id,order_id,anonymized_at) VALUES($1,$2,$3::text::timestamptz)")
                        .bind(self.tenant.id()).bind(&id).bind(stamp(request.now)).execute(&mut *tx).await.map_err(storage_error)?;
                    sqlx::query("UPDATE fl_ecommerce.records SET data=jsonb_set(data,'{updatedAt}',to_jsonb($3::text)) WHERE tenant_id=$1 AND kind='order' AND id=$2")
                        .bind(self.tenant.id()).bind(&id).bind(stamp(request.now)).execute(&mut *tx).await.map_err(storage_error)?;
                }
            }
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(count)
    }
}
