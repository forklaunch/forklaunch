//! Current CAC module uses MockProcedureCodeProvider and owns no durable storage.
//! Replace this declaration when adding persistence or a patient-data store.
use forklaunch_core::persistence::compliance::ComplianceEntity;

pub fn declarations() -> Vec<ComplianceEntity> {
    Vec::new()
}
