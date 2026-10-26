//! Explicit storage declaration hook for this health-only service.
//! This generated service owns no durable persisted storage. Its demonstration
//! in-memory repository is process-local. Add ComplianceMetadata storage entities
//! here when introducing a database, cache or object store; never infer coverage
//! from API projections or silently fall back to an empty declaration list.
use forklaunch_core::persistence::compliance::ComplianceEntity;

pub fn declarations() -> Vec<ComplianceEntity> {
    Vec::new()
}
