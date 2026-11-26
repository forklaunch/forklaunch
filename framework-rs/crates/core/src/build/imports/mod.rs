//! Offline, bounded import of existing TypeScript/OpenAPI contract assets.
mod delegated;
mod models;
mod openapi;
use crate::codegen::schema;

/// Internal reference table key retained in portable SDK schemas.
pub const SCHEMA_DEFINITIONS: &str = schema::DEFINITIONS;

/// Normalize a schema without fetching external resources.
/// # Errors
/// Rejects unsupported or oversized schema graphs and invalid reference scopes.
pub fn normalize_schema(
    schema: &serde_json::Value,
    prefix: &str,
) -> anyhow::Result<serde_json::Value> {
    self::schema::normalize(schema, prefix)
}

/// Import `OpenAPI` 3.0/3.1, including `ForkLaunch` version bundles, into a portable contract.
/// # Errors
/// Rejects unsupported media, references, operation identifiers and schema semantics.
pub fn contract_from_openapi(document: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    openapi::contract_from_openapi(document)
}
