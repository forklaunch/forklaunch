//! The exporter compiles the same protected domain model used by the repository.
#[allow(dead_code)]
#[path = "entities/stored_record.rs"]
mod stored_record;
pub use stored_record::StoredRecord;
use forklaunch_core::persistence::compliance::ComplianceEntity;
pub fn declarations() -> Vec<ComplianceEntity> {
    vec![ComplianceEntity::of::<StoredRecord>("records", "postgres-table")]
}
