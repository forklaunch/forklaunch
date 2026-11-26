//! Typed URL form decoding over the framework's bounded qs-compatible parser.
use super::{ContractValidationError, ContractValidationLocation};
use serde::de::{
    self, DeserializeOwned, IntoDeserializer, Visitor,
    value::{Error, MapDeserializer, SeqDeserializer},
};
use serde_json::Value;

/// Decode nested brackets, repeated fields and arrays using the shared form parser.
/// Scalar strings stay strings unless the Rust destination requests a number/bool.
/// Uses the framework's default 100 KiB, 1,000 parameter and 32-level limits.
/// # Errors
/// Rejects oversized/unsupported encodings, invalid nesting, or incompatible typed values.
pub fn decode_urlencoded<T: DeserializeOwned>(
    bytes: &[u8],
    content_type: &str,
) -> Result<T, ContractValidationError> {
    let value = super::versioning::parse_form_value(bytes, content_type)?;
    decode_form_value(value)
}

/// Decode typed URL form data with explicit shared native parser options.
/// # Errors
/// Rejects invalid parser settings, wire limits, nesting, or incompatible typed values.
pub fn decode_urlencoded_with_options<T: DeserializeOwned>(
    bytes: &[u8],
    content_type: &str,
    options: &crate::contract::BodyParserOptions,
) -> Result<T, ContractValidationError> {
    let value = super::versioning::parse_form_value_with_options(bytes, content_type, options)?;
    decode_form_value(value)
}

/// Decode an already parsed form tree using destination-directed scalar coercion.
/// This does not reparse wire data or replace the transport's parser limits.
/// # Errors
/// Rejects values incompatible with the destination Rust type.
pub fn decode_form_value<T: DeserializeOwned>(value: Value) -> Result<T, ContractValidationError> {
    T::deserialize(FormValue(value)).map_err(|_| {
        ContractValidationError::new(
            ContractValidationLocation::BodyParser,
            "invalid typed form body",
        )
    })
}

struct FormValue(Value);
impl IntoDeserializer<'_, Error> for FormValue {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}
macro_rules! scalar {
    ($method:ident, $ty:ty, $visit:ident) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            if let Value::String(value) = self.0 {
                let value = value
                    .parse::<$ty>()
                    .map_err(|_| de::Error::custom("invalid form scalar"))?;
                visitor.$visit(value)
            } else {
                self.deserialize_any(visitor)
            }
        }
    };
}
impl<'de> de::Deserializer<'de> for FormValue {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Object(values) => visitor.visit_map(MapDeserializer::new(
                values
                    .into_iter()
                    .map(|(key, value)| (key, FormValue(value))),
            )),
            Value::Array(values) => {
                visitor.visit_seq(SeqDeserializer::new(values.into_iter().map(FormValue)))
            }
            value => value.deserialize_any(visitor).map_err(de::Error::custom),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.0.is_null() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            Value::String(value) => visitor.visit_enum(value.into_deserializer()),
            Value::Object(values) => de::value::MapAccessDeserializer::new(MapDeserializer::new(
                values
                    .into_iter()
                    .map(|(key, value)| (key, FormValue(value))),
            ))
            .deserialize_enum(name, variants, visitor),
            _ => Err(de::Error::custom("invalid form enum")),
        }
    }
    scalar!(deserialize_bool, bool, visit_bool);
    scalar!(deserialize_i8, i8, visit_i8);
    scalar!(deserialize_i16, i16, visit_i16);
    scalar!(deserialize_i32, i32, visit_i32);
    scalar!(deserialize_i64, i64, visit_i64);
    scalar!(deserialize_i128, i128, visit_i128);
    scalar!(deserialize_u8, u8, visit_u8);
    scalar!(deserialize_u16, u16, visit_u16);
    scalar!(deserialize_u32, u32, visit_u32);
    scalar!(deserialize_u64, u64, visit_u64);
    scalar!(deserialize_u128, u128, visit_u128);
    scalar!(deserialize_f32, f32, visit_f32);
    scalar!(deserialize_f64, f64, visit_f64);
    serde::forward_to_deserialize_any! { char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any }
}

/// Encode a typed nested URL form using bracket paths and explicit array indices.
/// Empty collections and null have no lossless default Express form representation.
/// # Errors
/// Rejects unsupported values/keys, excessive depth, parameters or encoded bytes.
pub fn encode_urlencoded<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    let Value::Object(fields) = value else {
        return Err(form_encoding_error());
    };
    let mut pairs = Vec::new();
    for (name, value) in fields {
        valid_form_key(&name)?;
        encode_form_field(&name, &value, 0, &mut pairs)?;
    }
    let result = pairs.join("&");
    if result.len() > 102_400 {
        return Err(form_encoding_error());
    }
    Ok(result)
}
fn form_encoding_error() -> serde_json::Error {
    <serde_json::Error as serde::ser::Error>::custom("unsupported or oversized URL form")
}
fn valid_form_key(key: &str) -> Result<(), serde_json::Error> {
    if key.is_empty()
        || key == "__proto__"
        || key.bytes().all(|byte| byte.is_ascii_digit())
        || key.contains(['[', ']'])
    {
        Err(form_encoding_error())
    } else {
        Ok(())
    }
}
fn encode_form_field(
    path: &str,
    value: &Value,
    depth: usize,
    pairs: &mut Vec<String>,
) -> Result<(), serde_json::Error> {
    if depth > 32 || pairs.len() >= 1000 || path.len() > 102_400 {
        return Err(form_encoding_error());
    }
    match value {
        Value::Object(fields) if !fields.is_empty() => {
            for (name, value) in fields {
                valid_form_key(name)?;
                encode_form_field(&format!("{path}[{name}]"), value, depth + 1, pairs)?;
            }
        }
        Value::Array(values) if !values.is_empty() => {
            for (index, value) in values.iter().enumerate() {
                encode_form_field(&format!("{path}[{index}]"), value, depth + 1, pairs)?;
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {
            let value = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            if value.len() > 102_400 {
                return Err(form_encoding_error());
            }
            let key =
                percent_encoding::utf8_percent_encode(path, percent_encoding::NON_ALPHANUMERIC);
            let value =
                percent_encoding::utf8_percent_encode(&value, percent_encoding::NON_ALPHANUMERIC);
            pairs.push(format!("{key}={value}"));
            if pairs.iter().map(String::len).sum::<usize>() + pairs.len().saturating_sub(1)
                > 102_400
            {
                return Err(form_encoding_error());
            }
        }
        _ => return Err(form_encoding_error()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, serde::Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Profile {
        name: String,
        ages: Vec<u16>,
        enabled: bool,
    }
    #[derive(Debug, serde::Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Form {
        profile: Profile,
        labels: Vec<String>,
    }
    #[test]
    fn nested_forms_preserve_strings_and_coerce_only_typed_scalars() {
        let actual: Form = decode_urlencoded(b"profile[name]=007&profile[ages][]=2&profile[ages][]=3&profile[enabled]=true&labels=one&labels=two", "application/x-www-form-urlencoded").unwrap();
        assert_eq!(
            actual,
            Form {
                profile: Profile {
                    name: "007".into(),
                    ages: vec![2, 3],
                    enabled: true
                },
                labels: vec!["one".into(), "two".into()]
            }
        );
        for bytes in [
            b"profile[ages][]=70000".as_slice(),
            b"profile[enabled]=maybe",
            b"unknown=value",
        ] {
            assert!(decode_urlencoded::<Form>(bytes, "application/x-www-form-urlencoded").is_err());
        }
    }
    #[test]
    fn form_bounds_and_prototype_keys_are_shared_with_version_parser() {
        assert!(
            decode_urlencoded::<Value>(&vec![b'a'; 102_401], "application/x-www-form-urlencoded")
                .is_err()
        );
        assert!(
            decode_urlencoded::<Value>(
                "a=1&".repeat(1001).as_bytes(),
                "application/x-www-form-urlencoded"
            )
            .is_err()
        );
        assert!(
            decode_urlencoded::<Value>(
                format!("a{}=v", "[b]".repeat(33)).as_bytes(),
                "application/x-www-form-urlencoded"
            )
            .is_err()
        );
        let value: Value = decode_urlencoded(
            b"__proto__[polluted]=true&safe=value",
            "application/x-www-form-urlencoded",
        )
        .unwrap();
        assert_eq!(value, serde_json::json!({"safe":"value"}));
        assert!(
            decode_urlencoded::<Value>(b"x=1", "application/x-www-form-urlencoded; charset=utf-16")
                .is_err()
        );
    }
    #[test]
    fn nested_encoder_roundtrips_and_rejects_lossy_forms() {
        let value = serde_json::json!({"profile":{"name":"007 💙", "ages":[1,2], "enabled":true}, "labels":["a+b", "c&d"]});
        let encoded = encode_urlencoded(&value).unwrap();
        let decoded: Value =
            decode_urlencoded(encoded.as_bytes(), "application/x-www-form-urlencoded").unwrap();
        assert_eq!(decoded["profile"]["name"], "007 💙");
        assert_eq!(decoded["profile"]["ages"], serde_json::json!(["1", "2"]));
        assert_eq!(decoded["labels"], value["labels"]);
        for rejected in [
            serde_json::json!({"a":null}),
            serde_json::json!({"a":{"0":"ambiguous"}}),
            serde_json::json!({"a":[]}),
            serde_json::json!({"a":{}}),
            serde_json::json!({"a[b]":1}),
            serde_json::json!({"__proto__":1}),
            serde_json::json!({"a":"x".repeat(102_401)}),
        ] {
            assert!(encode_urlencoded(&rejected).is_err());
        }
    }
}
