//! Portable UTC timestamps with millisecond precision and native chrono values.
//!
//! Direct native date shapes are recognized; aliases require an explicit codec.
//! UTC input accepts zero to three fractional digits. Output always uses `.sssZ`.
//! No timestamp is silently rounded, truncated, or converted from an offset.
//!
//! ```
//! use forklaunch_core::contract::{schema, schema::Optional};
//! use forklaunch_core::contract::datetime::{DateTime, Utc, DateCodec};
//! type Stamp = DateTime<Utc>;
//! #[schema]
//! struct Event {
//!     times: Vec<Option<DateTime<Utc>>>,
//!     pair: [DateTime<Utc>; 2],
//!     #[field(optional, codec = DateCodec::UtcMilliseconds)]
//!     changed: Optional<Option<Stamp>>,
//! }
//! ```
//!
//! ```compile_fail
//! use forklaunch_core::contract::{schema, datetime::{DateTime, Utc}};
//! enum DateCodec { UtcMilliseconds }
//! #[schema]
//! struct Invalid {
//!     #[field(codec = DateCodec::UtcMilliseconds)]
//!     time: DateTime<Utc>,
//! }
//! ```
//!
//! ```compile_fail
//! use forklaunch_core::contract::{schema, datetime::DateCodec};
//! #[schema]
//! struct Invalid {
//!     #[field(codec = DateCodec::UtcMilliseconds)]
//!     time: String,
//! }
//! ```
//!
//! ```compile_fail
//! use forklaunch_core::contract::{schema, schema::Optional, datetime::{DateTime,Utc}};
//! #[schema]
//! struct Invalid { time: Optional<DateTime<Utc>> }
//! ```
pub use chrono::{DateTime, Utc};
use chrono::{Datelike, SecondsFormat, Timelike};

/// Parse UTC RFC3339 with zero to three fractional digits.
///
/// # Errors
/// Rejects offsets, leap seconds, invalid dates, and precision above milliseconds.
pub fn parse(value: &str) -> Result<DateTime<Utc>, &'static str> {
    let bytes = value.as_bytes();
    let valid_length = bytes.len() == 20 || (22..=24).contains(&bytes.len());
    if !valid_length || bytes.last() != Some(&b'Z') {
        return Err("expected UTC RFC3339 timestamp with millisecond precision");
    }
    for (index, byte) in bytes.iter().enumerate().take(bytes.len() - 1) {
        let expected = match index {
            4 | 7 => Some(b'-'),
            10 => Some(b'T'),
            13 | 16 => Some(b':'),
            19 => Some(b'.'),
            _ => None,
        };
        if expected.map_or_else(|| !byte.is_ascii_digit(), |expected| *byte != expected) {
            return Err("invalid UTC timestamp syntax");
        }
    }
    let date = DateTime::parse_from_rfc3339(value)
        .map_err(|_| "invalid UTC timestamp")?
        .with_timezone(&Utc);
    validate(&date)?;
    Ok(date)
}

/// Check that a native timestamp has a lossless portable representation.
///
/// # Errors
/// Rejects extended years, leap seconds and submillisecond timestamps.
pub fn validate(value: &DateTime<Utc>) -> Result<(), &'static str> {
    if !(0..=9999).contains(&value.year())
        || value.nanosecond() >= 1_000_000_000
        || !value.nanosecond().is_multiple_of(1_000_000)
    {
        return Err("timestamp is outside the UTC millisecond contract");
    }
    Ok(())
}

/// Serialize a native timestamp without rounding or truncating it.
///
/// # Errors
/// Rejects any value without a lossless portable representation.
pub fn serialize<S: serde::Serializer, T: DateShape>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let value = value
        .encode_date_shape()
        .map_err(serde::ser::Error::custom)?;
    serde::Serialize::serialize(&value, serializer)
}

/// Deserialize a checked native date shape.
///
/// # Errors
/// Rejects malformed timestamps or a value that differs from the declared shape.
pub fn deserialize<'de, D: serde::Deserializer<'de>, T: DateShape>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    T::decode_date_shape(value).map_err(serde::de::Error::custom)
}

/// Explicit codec for aliased native date shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateCodec {
    /// UTC timestamps without precision loss.
    UtcMilliseconds,
}

/// Checked native date containers used by generated wire codecs.
/// Application DTOs keep chrono and standard collection types.
#[doc(hidden)]
pub trait DateShape: Sized + sealed::DateShape {
    /// # Errors
    /// Rejects invalid date values and incompatible container shapes.
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String>;
    /// # Errors
    /// Rejects timestamps that cannot be represented without precision loss.
    fn encode_date_shape(&self) -> Result<serde_json::Value, String>;
    #[cfg(feature = "typed-schema")]
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema;
}
impl DateShape for DateTime<Utc> {
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String> {
        parse(value.as_str().ok_or("expected timestamp string")?).map_err(str::to_owned)
    }
    fn encode_date_shape(&self) -> Result<serde_json::Value, String> {
        validate(self).map_err(str::to_owned)?;
        Ok(serde_json::Value::String(
            self.to_rfc3339_opts(SecondsFormat::Millis, true),
        ))
    }
    #[cfg(feature = "typed-schema")]
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schema(generator)
    }
}
impl<T: DateShape> DateShape for Option<T> {
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String> {
        if value.is_null() {
            Ok(None)
        } else {
            T::decode_date_shape(value).map(Some)
        }
    }
    fn encode_date_shape(&self) -> Result<serde_json::Value, String> {
        self.as_ref()
            .map_or(Ok(serde_json::Value::Null), T::encode_date_shape)
    }
    #[cfg(feature = "typed-schema")]
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "anyOf": [T::date_shape_schema(generator), { "type": "null" }] })
    }
}
impl<T: DateShape> DateShape for Vec<T> {
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String> {
        let serde_json::Value::Array(values) = value else {
            return Err("expected date array".into());
        };
        values.into_iter().map(T::decode_date_shape).collect()
    }
    fn encode_date_shape(&self) -> Result<serde_json::Value, String> {
        self.iter()
            .map(T::encode_date_shape)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array)
    }
    #[cfg(feature = "typed-schema")]
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "array", "items": T::date_shape_schema(generator) })
    }
}
impl<T: DateShape, const N: usize> DateShape for [T; N] {
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String> {
        Vec::<T>::decode_date_shape(value)?
            .try_into()
            .map_err(|_| "wrong date array length".into())
    }
    fn encode_date_shape(&self) -> Result<serde_json::Value, String> {
        self.iter()
            .map(T::encode_date_shape)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array)
    }
    #[cfg(feature = "typed-schema")]
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = Vec::<T>::date_shape_schema(generator);
        let Ok(limit) = u32::try_from(N) else {
            return false.into();
        };
        schema.insert("minItems".into(), limit.into());
        schema.insert("maxItems".into(), limit.into());
        schema
    }
}
#[cfg(feature = "typed-schema")]
impl<T: DateShape> DateShape for super::schema::Optional<T> {
    fn decode_date_shape(value: serde_json::Value) -> Result<Self, String> {
        T::decode_date_shape(value).map(Self::Value)
    }
    fn encode_date_shape(&self) -> Result<serde_json::Value, String> {
        match self {
            Self::Missing => Err("missing date member must be omitted".into()),
            Self::Value(value) => value.encode_date_shape(),
        }
    }
    fn date_shape_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        T::date_shape_schema(generator)
    }
}
/// Schema for a checked native date or date container.
#[cfg(feature = "typed-schema")]
#[must_use]
pub fn shape_schema<T: DateShape>(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    T::date_shape_schema(generator)
}

/// The canonical wire schema and explicit runtime codec identity.
#[cfg(feature = "typed-schema")]
#[must_use]
pub fn schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "format": "date-time",
        "pattern": r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-5][0-9]\.[0-9]{3}Z$",
        "x-forklaunch-date-codec": {"version":1,"projection":"value","kind":"utc-milliseconds"},
    })
}

/// Type-check explicit alias codec policy in generated schema declarations.
#[cfg(feature = "typed-schema")]
#[doc(hidden)]
#[must_use]
pub fn checked_shape_schema<T: DateShape, const CHECK: bool>(
    generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    let _ = CHECK;
    T::date_shape_schema(generator)
}

mod sealed {
    pub trait DateShape {}
    impl DateShape for super::DateTime<super::Utc> {}
    impl<T: super::DateShape> DateShape for Option<T> {}
    impl<T: super::DateShape> DateShape for Vec<T> {}
    impl<T: super::DateShape, const N: usize> DateShape for [T; N] {}
    #[cfg(feature = "typed-schema")]
    impl<T: super::DateShape> DateShape for super::super::schema::Optional<T> {}
}

/// Generated object-member omission adapter: supplied null remains distinct from missing.
#[doc(hidden)]
pub mod optional {
    use super::DateShape;
    /// # Errors
    /// Rejects invalid dates and serialization of an unomitted missing member.
    #[allow(clippy::ref_option)] // Serde with-modules require borrowing the original field type.
    pub fn serialize<S: serde::Serializer, T: DateShape>(
        value: &Option<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let value = value
            .as_ref()
            .ok_or_else(|| serde::ser::Error::custom("missing date member must be omitted"))?;
        super::serialize(value, serializer)
    }
    /// # Errors
    /// Rejects invalid dates and null unless the inner declared type is nullable.
    pub fn deserialize<'de, D: serde::Deserializer<'de>, T: DateShape>(
        deserializer: D,
    ) -> Result<Option<T>, D::Error> {
        super::deserialize(deserializer).map(Some)
    }
}
