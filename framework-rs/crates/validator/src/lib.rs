#![forbid(unsafe_code)]

//! Validation and schema integration for the Rust framework.
//!
//! Combines the three standard building blocks `ForkLaunch` relies on:
//!
//! - [`serde::de::DeserializeOwned`] for parsing wire input into Rust types,
//! - [`garde::Validate`] for runtime validation,
//! - [`schemars::JsonSchema`] for schema metadata used by OpenAPI/SDK codegen.
//!
//! Parse and validation failures are normalized into
//! [`ContractValidationError`] so they flow through the same error model as the
//! rest of the framework.
//!
//! ## Coercion policy (where Rust is intentionally stricter than TypeScript)
//!
//! Path and query parameters arrive as strings and may be coerced via the
//! [`coerce`] helpers. `ForkLaunch` deliberately does **not** copy some of the
//! looser TypeScript coercions:
//!
//! - Booleans accept only `true`/`false`/`1`/`0` — not `yes`/`on`/`""`.
//! - Numbers reject empty strings, surrounding whitespace, and `NaN`/`Infinity`.
//! - UUIDs and timestamps must be fully valid (`RFC 4122` / `RFC 3339`).

use forklaunch_common::{ContractValidationError, ContractValidationLocation};
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};

mod parse_errors;
pub mod typescript_coerce;
pub mod zod_array;
pub mod zod_provenance;
pub use parse_errors::{ParseError, pretty_print_parse_errors};

/// A type usable as a `ForkLaunch` request/response schema.
///
/// Blanket-implemented for every type that is deserializable, serializable,
/// JSON-schema-describable, and validatable with a default (`()`) context.
pub trait FrameworkSchema:
    DeserializeOwned + Serialize + JsonSchema + garde::Validate<Context = ()>
{
    /// Returns the JSON Schema for this type as a compact JSON string.
    #[must_use]
    fn json_schema_json() -> String {
        // Draft-07, the dialect the contract validators and TypeScript share.
        let schema = schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Self>();
        serde_json::to_string(&schema).unwrap_or_default()
    }
}

impl<T> FrameworkSchema for T where
    T: DeserializeOwned + Serialize + JsonSchema + garde::Validate<Context = ()>
{
}

/// Parses a JSON body into `T`, mapping parse failures to a body validation error.
///
/// # Errors
///
/// Returns a single body-level [`ContractValidationError`] if the bytes are not
/// valid JSON for `T`.
pub fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Vec<ContractValidationError>> {
    serde_json::from_slice(bytes).map_err(|error| {
        vec![ContractValidationError::new(
            ContractValidationLocation::Body,
            format!("invalid JSON body: {error}"),
        )]
    })
}

/// Runs `garde` validation, mapping each failing field to a validation error.
///
/// # Errors
///
/// Returns one [`ContractValidationError`] per failed field rule.
pub fn validate_value<T: garde::Validate<Context = ()>>(
    value: &T,
) -> Result<(), Vec<ContractValidationError>> {
    value.validate().map_err(|report| report_to_errors(&report))
}

/// Parses and then validates a JSON body in one step.
///
/// # Errors
///
/// Returns body parse errors, or — when parsing succeeds — any field validation
/// errors produced by `garde`.
pub fn parse_and_validate<T>(bytes: &[u8]) -> Result<T, Vec<ContractValidationError>>
where
    T: DeserializeOwned + garde::Validate<Context = ()>,
{
    let value = parse_json::<T>(bytes)?;
    validate_value(&value)?;
    Ok(value)
}

fn report_to_errors(report: &garde::Report) -> Vec<ContractValidationError> {
    report
        .iter()
        .map(|(path, error)| {
            ContractValidationError::new(
                ContractValidationLocation::Body,
                format!("{path}: {error}"),
            )
            .with_field(path.to_string())
        })
        .collect()
}

/// String-to-typed coercion for path and query parameters.
pub mod coerce {
    use forklaunch_common::{ContractValidationError, ContractValidationLocation};

    fn error(
        location: ContractValidationLocation,
        field: &str,
        message: impl Into<String>,
    ) -> ContractValidationError {
        ContractValidationError::new(location, message.into()).with_field(field.to_owned())
    }

    /// Coerces `true`/`false`/`1`/`0` into a boolean (case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns an error for any other value, including `yes`/`on`/empty strings.
    pub fn boolean(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<bool, ContractValidationError> {
        match raw.trim() {
            "1" => Ok(true),
            "0" => Ok(false),
            other if other.eq_ignore_ascii_case("true") => Ok(true),
            other if other.eq_ignore_ascii_case("false") => Ok(false),
            _ => Err(error(
                location,
                field,
                format!("`{field}` must be a boolean (true/false/1/0)"),
            )),
        }
    }

    /// Coerces a base-10 signed integer.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is empty, padded, or not an integer.
    pub fn integer(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<i64, ContractValidationError> {
        reject_padding(location, field, raw)?;
        raw.parse::<i64>()
            .map_err(|_| error(location, field, format!("`{field}` must be an integer")))
    }

    /// Coerces a base-10 unsigned integer.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is empty, padded, negative, or not an integer.
    pub fn unsigned(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<u64, ContractValidationError> {
        reject_padding(location, field, raw)?;
        raw.parse::<u64>().map_err(|_| {
            error(
                location,
                field,
                format!("`{field}` must be a non-negative integer"),
            )
        })
    }

    /// Coerces a finite base-10 floating point number.
    ///
    /// # Errors
    ///
    /// Returns an error for empty/padded input or for non-finite values such as
    /// `NaN` and `Infinity`.
    pub fn number(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<f64, ContractValidationError> {
        reject_padding(location, field, raw)?;
        match raw.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(value),
            _ => Err(error(
                location,
                field,
                format!("`{field}` must be a finite number"),
            )),
        }
    }

    /// Coerces an `RFC 4122` UUID.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is not a valid UUID.
    pub fn uuid(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<uuid::Uuid, ContractValidationError> {
        uuid::Uuid::parse_str(raw.trim())
            .map_err(|_| error(location, field, format!("`{field}` must be a valid UUID")))
    }

    /// Coerces an `RFC 3339` timestamp into a UTC datetime.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is not a valid RFC 3339 timestamp.
    pub fn datetime(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<chrono::DateTime<chrono::Utc>, ContractValidationError> {
        raw.trim()
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map_err(|_| {
                error(
                    location,
                    field,
                    format!("`{field}` must be an RFC 3339 timestamp"),
                )
            })
    }

    fn reject_padding(
        location: ContractValidationLocation,
        field: &str,
        raw: &str,
    ) -> Result<(), ContractValidationError> {
        if raw.is_empty() || raw.trim() != raw {
            return Err(error(
                location,
                field,
                format!("`{field}` must not be empty or padded with whitespace"),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generic_json_requests_preserve_literal_serde_marker_keys() {
        #[derive(serde::Deserialize)]
        struct Body {
            nested: serde_json::Value,
        }
        // A first key of `$serde_json::private::RawValue` is upstream serde_json's raw-value marker; it is not object data.
        {
            let key = "$serde_json::private::Number";
            let literal = serde_json::Value::Object(
                [(key.to_owned(), serde_json::json!({"value":true}))]
                    .into_iter()
                    .collect(),
            );
            let encoded = literal.to_string();
            assert_eq!(
                super::parse_json::<serde_json::Value>(encoded.as_bytes()).unwrap(),
                literal
            );
            let typed = format!("{{\"nested\":{encoded}}}");
            assert_eq!(
                super::parse_json::<Body>(typed.as_bytes()).unwrap().nested,
                literal
            );
        }
    }

    use forklaunch_common::ContractValidationLocation;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Deserialize, Serialize, JsonSchema, garde::Validate)]
    struct CreateUser {
        #[garde(length(min = 1, max = 50))]
        name: String,
        #[garde(range(min = 0, max = 130))]
        age: u8,
    }

    #[test]
    fn parses_and_validates_valid_body() {
        let user = parse_and_validate::<CreateUser>(br#"{"name":"Ada","age":36}"#)
            .expect("valid body should parse");
        assert_eq!(user.name, "Ada");
        assert_eq!(user.age, 36);
    }

    #[test]
    fn reports_validation_errors_per_field() {
        let errors = parse_and_validate::<CreateUser>(br#"{"name":"","age":200}"#)
            .expect_err("invalid body should fail");
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|error| {
            error
                .field
                .as_deref()
                .is_some_and(|field| field.contains("name"))
        }));
        assert!(errors.iter().any(|error| {
            error
                .field
                .as_deref()
                .is_some_and(|field| field.contains("age"))
        }));
    }

    #[test]
    fn reports_parse_errors_as_body_errors() {
        let errors =
            parse_and_validate::<CreateUser>(b"not json").expect_err("invalid json should fail");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].location, ContractValidationLocation::Body);
    }

    #[test]
    fn exposes_json_schema_and_schema_ref() {
        let schema = CreateUser::json_schema_json();
        assert!(schema.contains("\"name\""));
        assert!(schema.contains("\"age\""));

        assert_eq!(CreateUser::schema_name(), "CreateUser");
    }

    #[test]
    fn coerces_strictly() {
        use ContractValidationLocation::Query;

        assert!(coerce::boolean(Query, "active", "true").unwrap());
        assert!(!coerce::boolean(Query, "active", "0").unwrap());
        assert!(coerce::boolean(Query, "active", "yes").is_err());
        assert!(coerce::boolean(Query, "active", "").is_err());

        assert_eq!(coerce::integer(Query, "limit", "10").unwrap(), 10);
        assert!(coerce::integer(Query, "limit", " 10 ").is_err());
        assert!(coerce::unsigned(Query, "limit", "-1").is_err());
        assert!(coerce::number(Query, "ratio", "NaN").is_err());

        assert!(coerce::uuid(Query, "id", "550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(coerce::uuid(Query, "id", "nope").is_err());
        assert!(coerce::datetime(Query, "since", "2026-06-16T00:00:00Z").is_ok());
        assert!(coerce::datetime(Query, "since", "yesterday").is_err());
    }
}
