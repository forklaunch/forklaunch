//! Explicit checked mapping boundaries, independent of HTTP request handling.
use crate::{
    common::{FrameworkError, FrameworkResult},
    contract::{
        schema_validation::{CheckedJsonValidator, SchemaValidationFailure, compile_checked},
        schema_value_for,
    },
};
use schemars::JsonSchema;
use serde::Serialize;
use std::{future::Future, marker::PhantomData, sync::Arc};

/// A precompiled DTO boundary. Business mapping remains an explicit Rust function.
/// Validates the value projection without inventing or replaying transformations.
/// Not a storage authorization, transaction or encryption boundary.
pub struct CheckedMapper<T> {
    validator: Arc<CheckedJsonValidator>,
    marker: PhantomData<fn() -> T>,
}
impl<T: JsonSchema + Serialize> CheckedMapper<T> {
    /// Compile the canonical DTO value schema once during registration.
    /// # Errors
    /// Rejects unsupported or invalid schemas and validator resource limits.
    pub fn new() -> FrameworkResult<Self> {
        let validator = compile_checked(&schema_value_for::<T>()).map_err(|_| {
            FrameworkError::new("invalid_mapper_schema", "Invalid mapper value schema")
        })?;
        Ok(Self {
            validator,
            marker: PhantomData,
        })
    }
    /// Check an already-decoded DTO before invoking application mapping logic.
    /// # Errors
    /// Rejects invalid values, serialization failures and evaluator resource limits.
    pub async fn to_entity<E, F, Fut>(&self, dto: T, map: F) -> FrameworkResult<E>
    where
        F: FnOnce(T) -> Fut,
        Fut: Future<Output = FrameworkResult<E>>,
    {
        self.validate(&dto)?;
        map(dto).await
    }
    /// Check the mapped DTO before releasing it to a caller outside or inside HTTP.
    /// # Errors
    /// Propagates application errors and rejects invalid mapped values.
    pub async fn to_dto<E, F, Fut>(&self, entity: E, map: F) -> FrameworkResult<T>
    where
        F: FnOnce(E) -> Fut,
        Fut: Future<Output = FrameworkResult<T>>,
    {
        let dto = map(entity).await?;
        self.validate(&dto)?;
        Ok(dto)
    }
    fn validate(&self, dto: &T) -> FrameworkResult<()> {
        let error = || FrameworkError::new("invalid_mapper_value", "Invalid mapper value");
        let value = serde_json::to_value(dto).map_err(|_| error())?;
        (self.validator)(&value).map_err(|failure| match failure {
            SchemaValidationFailure::Invalid(_) => error(),
            SchemaValidationFailure::Operational => FrameworkError::new(
                "mapper_validation_unavailable",
                "Mapper validation resource limit",
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operational_failure_is_not_an_invalid_value() {
        let mapper = CheckedMapper::<String> {
            validator: Arc::new(|_| Err(SchemaValidationFailure::Operational)),
            marker: PhantomData,
        };
        assert_eq!(
            mapper
                .validate(&"private value".to_owned())
                .unwrap_err()
                .code(),
            "mapper_validation_unavailable"
        );
    }
}
