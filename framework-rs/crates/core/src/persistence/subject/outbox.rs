//! Transactional external-erasure work. Explicitly enumerate targets in the same
//! SQL transaction as source deletion. Completion covers only that sealed set;
//! adapters remain responsible for storage holds, ownership, versions and backups.
use super::postgres::SubjectEraseAudit;
use crate::persistence::begin_tenant_transaction;
include!("outbox_shared.rs");
