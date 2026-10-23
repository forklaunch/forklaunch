//! Durable encrypted materialized exports. Services supply an authorized source
//! projection captured consistently; this module does not infer subject ownership
//! or query unrelated storage. Fixed pages survive process restarts and source
//! changes. The PostgreSQL mailbox is one concrete destination, not a claim that
//! arbitrary third-party delivery, expiry deletion or cross-store erasure exists.
use crate::persistence::begin_tenant_transaction;
include!("export_shared.rs");
