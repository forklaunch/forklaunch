//! Successful unversioned schema field projections, independent of wire values.
use crate::http::{Headers, VersionFieldValues};
use serde_json::Value;

/// Reuses the existing parameter/query/header value-map representation.
pub type RequestFieldValues = VersionFieldValues;

/// Atomic output from a trusted whole-request schema callback.
/// `body` is the successful JSON result (including absent/root removal) when the
/// route declares a body. Bodyless routes must return `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestTransformOutput {
    pub fields: RequestFieldValues,
    pub body: Option<Value>,
}

/// Fields accepted by a registered unversioned whole-request schema callback.
/// Absent on failed None/Warning validation: original request maps and raw query
/// remain available. This is not a selected version or a generic schema engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRequestFields {
    values: RequestFieldValues,
}
impl ParsedRequestFields {
    /// Trust a complete successful schema result. Existing header keys are kept;
    /// only truthy values for those exact keys replace wire strings. Absent-header
    /// defaults are not inserted. Credential extraction still uses wire headers.
    #[must_use]
    pub fn from_validated(mut values: RequestFieldValues, original_headers: &Headers) -> Self {
        values.headers = values.project_wire_headers(original_headers);
        Self { values }
    }

    #[must_use]
    pub const fn values(&self) -> &RequestFieldValues {
        &self.values
    }
}
