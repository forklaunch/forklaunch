//! `PostgreSQL` pool admission under the `postgres09` name, kept for existing
//! callers (including Apalis workers). `postgres` now uses `SQLx` 0.9 too, so
//! this is the same module.
pub use super::postgres::*;
