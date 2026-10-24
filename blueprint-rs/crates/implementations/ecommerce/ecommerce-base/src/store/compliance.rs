use super::{InMemoryEcommerceStore, Record, RecordKind, storage_error};
use crate::compliance::{
    EcommerceComplianceStore, OrderExport, OrderSubjectField, validate_subject,
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::OrderDto;

fn selected(order: &OrderDto, subject: &str, field: OrderSubjectField) -> bool {
    match field {
        OrderSubjectField::Id => order.id == subject,
        OrderSubjectField::CustomerId => order.customer_id.as_deref() == Some(subject),
    }
}
impl EcommerceComplianceStore for InMemoryEcommerceStore {
    async fn export_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> BlueprintResult<Vec<OrderExport>> {
        validate_subject(subject)?;
        let records = self.records.lock().map_err(storage_error)?;
        records
            .values()
            .filter_map(|record| match record {
                Record::Order(order) if selected(order, subject, field) => {
                    Some(OrderExport::new(order.id.clone(), &order.shipping_address))
                }
                _ => None,
            })
            .collect()
    }
    async fn erase_orders(
        &self,
        subject: &str,
        field: OrderSubjectField,
    ) -> BlueprintResult<usize> {
        validate_subject(subject)?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let ids: Vec<_> = records
            .values()
            .filter_map(|record| match record {
                Record::Order(order) if selected(order, subject, field) => Some(order.id.clone()),
                _ => None,
            })
            .collect();
        let mut markers = self.retention_markers.lock().map_err(storage_error)?;
        for id in &ids {
            markers.remove(id);
            records.remove(&(RecordKind::Order, id.clone()));
        }
        Ok(ids.len())
    }
}
