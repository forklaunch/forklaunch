use super::{InMemoryEcommerceStore, Record, RecordKind};
use crate::retention::{storage_error, validate};
use forklaunch_core::{
    common::FrameworkResult,
    persistence::retention::{RetentionAction, RetentionRequest, RetentionStore},
};
use std::time::SystemTime;

fn created(record: &Record) -> FrameworkResult<Option<SystemTime>> {
    let Record::Order(order) = record else {
        return Ok(None);
    };
    order
        .created_at
        .as_deref()
        .map(|date| {
            chrono::DateTime::parse_from_rfc3339(date)
                .map(Into::into)
                .map_err(storage_error)
        })
        .transpose()
}
impl RetentionStore for InMemoryEcommerceStore {
    async fn count(&self, request: RetentionRequest<'_>) -> FrameworkResult<u64> {
        validate(request, self.retention_tenant.as_ref())?;
        let records = self.records.lock().map_err(storage_error)?;
        let markers = self.retention_markers.lock().map_err(storage_error)?;
        let mut count = 0;
        for record in records.values() {
            if created(record)?
                .is_some_and(|date| request.policy.eligible(date, markers.contains(record.id())))
            {
                count += 1;
            }
        }
        Ok(count)
    }
    async fn apply_batch(&self, request: RetentionRequest<'_>) -> FrameworkResult<usize> {
        validate(request, self.retention_tenant.as_ref())?;
        let mut records = self.records.lock().map_err(storage_error)?;
        let mut markers = self.retention_markers.lock().map_err(storage_error)?;
        let mut selected = std::collections::BTreeSet::new();
        for record in records.values() {
            if let Some(date) = created(record)?.filter(|date| {
                request
                    .policy
                    .eligible(*date, markers.contains(record.id()))
            }) {
                selected.insert((date, record.id().to_owned()));
                if selected.len() > request.batch_size {
                    selected.pop_last();
                }
            }
        }
        let count = selected.len();
        let now = chrono::DateTime::<chrono::Utc>::from(request.now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        for (_, id) in selected {
            match request.policy.action() {
                RetentionAction::Delete => {
                    records.remove(&(RecordKind::Order, id.clone()));
                    markers.remove(&id);
                }
                RetentionAction::Anonymize => {
                    if let Some(Record::Order(order)) =
                        records.get_mut(&(RecordKind::Order, id.clone()))
                    {
                        order.updated_at = Some(now.clone());
                    }
                    markers.insert(id);
                }
            }
        }
        Ok(count)
    }
}
