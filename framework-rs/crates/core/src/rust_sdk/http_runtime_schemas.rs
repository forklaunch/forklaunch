//! Rust emission uses the shared bounded runtime schema boundary.
pub(super) use crate::codegen::runtime_schemas::{fields, request_objects, schema, versions};

#[cfg(test)]
use serde_json::Value;

#[cfg(test)]
#[path = "http_runtime_schemas_tests.rs"]
mod tests;
