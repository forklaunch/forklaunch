use super::{
    Postgres, PostgresEcommerceStore, Record, RecordKind, Transaction, Value, storage_error,
};
use crate::compliance::{
    EcommerceComplianceStore, OrderExport, OrderSubjectField, validate_subject,
};
use forklaunch_core::domain::BlueprintResult;

impl PostgresEcommerceStore {
    async fn compliance_orders(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subject: &str,
        field: OrderSubjectField,
        erase: bool,
    ) -> BlueprintResult<Vec<OrderExport>> {
        validate_subject(subject)?;
        // Lock selected rows during erasure so customer reassignment cannot race
        // the scoped selection. All decrypt/audit work precedes deletion.
        let query = if erase {
            "SELECT id,data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND data->>$2=$3 ORDER BY id FOR UPDATE"
        } else {
            "SELECT id,data FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND data->>$2=$3 ORDER BY id"
        };
        let rows: Vec<(String, sqlx::types::Json<Value>)> = sqlx::query_as(query)
            .bind(self.tenant.id())
            .bind(field.as_str())
            .bind(subject)
            .fetch_all(&mut **tx)
            .await
            .map_err(storage_error)?;
        let mut output = Vec::with_capacity(rows.len());
        for (id, data) in rows {
            let Record::Order(order) = self
                .decode(
                    tx,
                    RecordKind::Order,
                    &id,
                    data.0,
                    if erase {
                        "compliance_erase"
                    } else {
                        "compliance_export"
                    },
                )
                .await?
            else {
                return Err(storage_error("invalid order"));
            };
            output.push(OrderExport::new(id, &order.shipping_address)?);
        }
        Ok(output)
    }
}
impl EcommerceComplianceStore for PostgresEcommerceStore {
    async fn export_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> BlueprintResult<Vec<OrderExport>> {
        let mut tx = self.transaction().await?;
        let rows = self
            .compliance_orders(&mut tx, subject, field, false)
            .await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(rows)
    }
    async fn erase_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> BlueprintResult<usize> {
        let mut tx = self.transaction().await?;
        let rows = self
            .compliance_orders(&mut tx, subject, field, true)
            .await?;
        for row in &rows {
            sqlx::query(
                "DELETE FROM fl_ecommerce.records WHERE tenant_id=$1 AND kind='order' AND id=$2",
            )
            .bind(self.tenant.id())
            .bind(&row.id)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(rows.len())
    }
}
