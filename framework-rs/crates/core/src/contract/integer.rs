//! Framework-owned codecs for explicitly bounded JSON integer fields.
use super::schema::Optional;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Only framework-supported integer shapes can use the generated codec.
pub trait IntegerValue {
    fn integer(&self) -> Option<i128>;
}
impl IntegerValue for i64 {
    fn integer(&self) -> Option<i128> {
        Some(i128::from(*self))
    }
}
impl IntegerValue for u64 {
    fn integer(&self) -> Option<i128> {
        Some(i128::from(*self))
    }
}
impl IntegerValue for usize {
    fn integer(&self) -> Option<i128> {
        Some(*self as i128)
    }
}
impl IntegerValue for isize {
    fn integer(&self) -> Option<i128> {
        Some(*self as i128)
    }
}
impl<T: IntegerValue> IntegerValue for Option<T> {
    fn integer(&self) -> Option<i128> {
        self.as_ref().and_then(IntegerValue::integer)
    }
}
impl<T: IntegerValue> IntegerValue for Optional<T> {
    fn integer(&self) -> Option<i128> {
        self.as_option().and_then(IntegerValue::integer)
    }
}
/// Decode a declared integer shape.
///
/// # Errors
/// Returns an error for malformed input or a value outside the declared bounds.
pub fn deserialize<'de, D, T, const MIN: i64, const MAX: i64>(
    deserializer: D,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + IntegerValue,
{
    let value = T::deserialize(deserializer)?;
    if value
        .integer()
        .is_some_and(|n| !(i128::from(MIN)..=i128::from(MAX)).contains(&n))
    {
        return Err(serde::de::Error::custom("integer violates contract bounds"));
    }
    Ok(value)
}
/// Encode a declared integer shape.
///
/// # Errors
/// Returns an error for a value outside the declared bounds or serializer failure.
pub fn serialize<S, T, const MIN: i64, const MAX: i64>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize + IntegerValue,
{
    if value
        .integer()
        .is_some_and(|n| !(i128::from(MIN)..=i128::from(MAX)).contains(&n))
    {
        return Err(serde::ser::Error::custom(
            "integer violates contract bounds",
        ));
    }
    value.serialize(serializer)
}

#[cfg(test)]
mod tests {
    #[crate::contract::schema]
    struct Counts {
        #[field(max = 10)]
        count: usize,
        #[field(min = -10, max = 10)]
        delta: isize,
    }
    #[test]
    fn bounded_platform_integers_validate_both_directions() {
        let value = serde_json::from_str::<Counts>(r#"{"count":10,"delta":-10}"#).unwrap();
        assert!(serde_json::to_value(value).is_ok());
        for input in [r#"{"count":11,"delta":0}"#, r#"{"count":0,"delta":-11}"#] {
            assert!(serde_json::from_str::<Counts>(input).is_err());
        }
        assert!(
            serde_json::to_value(Counts {
                count: 11,
                delta: 0
            })
            .is_err()
        );
        assert!(
            serde_json::to_value(Counts {
                count: 0,
                delta: 11
            })
            .is_err()
        );
    }
}
