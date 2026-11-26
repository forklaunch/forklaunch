//! Typed payload validation for the TypeScript WebSocket wire format.
use std::{fmt, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    Invalid(String),
    /// Bounded schema evaluation could not finish; this is not invalid data.
    Operational,
    Limit {
        max_bytes: usize,
    },
}
impl fmt::Display for PayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid WebSocket payload: {message}"),
            Self::Operational => {
                formatter.write_str("WebSocket schema evaluation could not be completed")
            }
            Self::Limit { max_bytes } => {
                write!(formatter, "WebSocket payload exceeds {max_bytes} bytes")
            }
        }
    }
}
impl std::error::Error for PayloadError {}

type Encoder<T> = fn(&T, usize, Option<&JsonValidator>) -> Result<Vec<u8>, PayloadError>;
type Decoder<T> = fn(&[u8], Option<&JsonValidator>) -> Result<T, PayloadError>;
pub(crate) type JsonValidator = dyn Fn(&Value) -> Result<(), PayloadError> + Send + Sync;
type Validator<T> = dyn Fn(&T) -> Result<(), String> + Send + Sync;

/// Encodes messages as raw UTF-8 strings or JSON bytes, matching the TS helpers.
/// Decoding first attempts JSON, then falls back to a lossy UTF-8 string.
/// Use `with_json_encoding` for strict JSON media instead, including strings/null.
/// A validator runs after decoding and before encoding; byte limits apply in both directions.
pub struct PayloadCodec<T> {
    encoder: Encoder<T>,
    decoder: Decoder<T>,
    validator: Arc<Validator<T>>,
    json_validator: Option<Arc<JsonValidator>>,
    max_bytes: usize,
}
impl<T> Clone for PayloadCodec<T> {
    fn clone(&self) -> Self {
        Self {
            encoder: self.encoder,
            decoder: self.decoder,
            validator: self.validator.clone(),
            json_validator: self.json_validator.clone(),
            max_bytes: self.max_bytes,
        }
    }
}
impl<T: Serialize + DeserializeOwned> PayloadCodec<T> {
    #[must_use]
    pub fn serde() -> Self {
        Self {
            encoder: encode_serde::<T, false>,
            decoder: decode_serde::<T, false>,
            validator: Arc::new(|_| Ok(())),
            json_validator: None,
            max_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
        }
    }

    /// Encode every value as JSON, including strings and null. Decode strictly
    /// as UTF-8 JSON without the TypeScript raw-string fallback.
    #[must_use]
    pub fn with_json_encoding(mut self) -> Self {
        self.encoder = encode_serde::<T, true>;
        self.decoder = decode_serde::<T, true>;
        self
    }

    /// Compile a JSON Schema once and validate original incoming values before
    /// deserialization and original serialized values before wire encoding.
    /// Missing dialect declarations use Draft 7. Email uses AJV full-format
    /// semantics; UUID assertions also apply in Draft 7. Decimal multiples are
    /// exact. Safe integral decimal/exponent forms also decode as typed integers.
    /// No HTTP/file retrieval is enabled, including through unified features.
    ///
    /// # Errors
    /// Rejects invalid schemas, unresolved resources and schema/regex budget limits.
    #[cfg(feature = "schema-validation")]
    pub fn json_schema(schema: &Value) -> Result<Self, PayloadError> {
        let mut codec = Self::serde();
        codec.json_validator = Some(crate::ws::codec_schema::compile(schema)?);
        Ok(codec)
    }

    /// Use the framework's garde validation in both directions.
    #[cfg(feature = "ws-validation")]
    #[must_use]
    pub fn framework() -> Self
    where
        T: forklaunch_validator::FrameworkSchema,
    {
        Self::serde().with_validator(|value| {
            forklaunch_validator::validate_value(value).map_err(|errors| format!("{errors:?}"))
        })
    }
}
impl PayloadCodec<Vec<u8>> {
    /// Explicit binary payloads bypass JSON/text interpretation.
    #[must_use]
    pub fn bytes() -> Self {
        Self {
            encoder: |value, limit, _| {
                check_limit(value.len(), limit)?;
                Ok(value.clone())
            },
            decoder: |bytes, _| Ok(bytes.to_vec()),
            validator: Arc::new(|_| Ok(())),
            json_validator: None,
            max_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
        }
    }
}
impl<T> PayloadCodec<T> {
    #[must_use]
    pub fn with_validator(
        mut self,
        validate: impl Fn(&T) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        self.validator = Arc::new(validate);
        self
    }

    /// # Errors
    /// Rejects zero limits.
    pub fn with_max_bytes(mut self, max_bytes: usize) -> Result<Self, PayloadError> {
        if max_bytes == 0 {
            return Err(PayloadError::Invalid(
                "payload limit must be positive".into(),
            ));
        }
        self.max_bytes = max_bytes;
        Ok(self)
    }

    #[must_use]
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// # Errors
    /// Rejects invalid values, serialization failures and excessive bytes. Raw-string
    /// compatibility mode additionally rejects null messages.
    pub fn encode(&self, value: &T) -> Result<Vec<u8>, PayloadError> {
        (self.validator)(value).map_err(PayloadError::Invalid)?;
        let bytes = (self.encoder)(value, self.max_bytes, self.json_validator.as_deref())?;
        check_limit(bytes.len(), self.max_bytes)?;
        Ok(bytes)
    }

    /// # Errors
    /// Rejects excessive bytes, schema/type mismatches and failed validation.
    pub fn decode(&self, bytes: &[u8]) -> Result<T, PayloadError> {
        check_limit(bytes.len(), self.max_bytes)?;
        let value = (self.decoder)(bytes, self.json_validator.as_deref())?;
        (self.validator)(&value).map_err(PayloadError::Invalid)?;
        Ok(value)
    }
}

fn check_limit(bytes: usize, max_bytes: usize) -> Result<(), PayloadError> {
    if bytes > max_bytes {
        Err(PayloadError::Limit { max_bytes })
    } else {
        Ok(())
    }
}
fn decode_serde<T: DeserializeOwned, const JSON: bool>(
    bytes: &[u8],
    validate: Option<&JsonValidator>,
) -> Result<T, PayloadError> {
    let text = String::from_utf8_lossy(bytes);
    let value: Value = if JSON {
        serde_json::from_slice(bytes).map_err(|error| PayloadError::Invalid(error.to_string()))?
    } else {
        match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) if serde_json::from_str::<&serde_json::value::RawValue>(&text).is_ok() => {
                // RawValue checks JSON syntax iteratively, without numeric conversion
                // or a recursive Value tree. Resource/number failures must not turn
                // otherwise valid structured JSON into a schema-valid text message.
                return Err(PayloadError::Invalid(format!(
                    "JSON payload exceeds representation limits: {error}"
                )));
            }
            Err(_) => Value::String(text.into_owned()),
        }
    };
    if let Some(validate) = validate {
        let mut normalized = value.clone();
        normalize_integer_numbers(&mut normalized);
        validate(&normalized)?;
        // Preserve floating-point representation (including negative zero) when
        // the target supports it; typed integers also accept safe integral forms.
        return serde_json::from_value(value)
            .or_else(|_| serde_json::from_value(normalized))
            .map_err(|error| PayloadError::Invalid(error.to_string()));
    }
    serde_json::from_value(value).map_err(|error| PayloadError::Invalid(error.to_string()))
}
fn encode_serde<T: Serialize, const JSON: bool>(
    value: &T,
    max_bytes: usize,
    validate: Option<&JsonValidator>,
) -> Result<Vec<u8>, PayloadError> {
    // JSON escaping can expand raw strings sixfold; the final wire bytes retain the exact limit.
    let mut output = LimitedBytes {
        bytes: Vec::new(),
        limit: if JSON {
            max_bytes
        } else {
            max_bytes.saturating_mul(6).saturating_add(2)
        },
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut output, value);
    if output.exceeded {
        return Err(PayloadError::Limit { max_bytes });
    }
    result.map_err(|error| PayloadError::Invalid(error.to_string()))?;
    let mut serialized = serde_json::from_slice(&output.bytes)
        .map_err(|error| PayloadError::Invalid(error.to_string()))?;
    if let Some(validate) = validate {
        normalize_integer_numbers(&mut serialized);
        validate(&serialized)?;
    }
    if JSON {
        return Ok(output.bytes);
    }
    match serialized {
        Value::String(value) => Ok(value.into_bytes()),
        Value::Null => Err(PayloadError::Invalid(
            "null cannot be sent as a message".into(),
        )),
        _ => Ok(output.bytes),
    }
}

// JavaScript JSON parsing does not retain an integer's decimal/exponent spelling.
// Canonicalize only exactly representable safe integers before schema validation
// and typed deserialization. Strings, fractions and larger numbers retain their
// existing representation and range rules.
#[allow(clippy::cast_possible_truncation)]
fn normalize_integer_numbers(value: &mut Value) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Number(number) if number.is_f64() => {
                if let Some(value) = number.as_f64()
                    && value.fract() == 0.0
                    && value.abs() <= 9_007_199_254_740_991.0
                {
                    *number = (value as i64).into();
                }
            }
            Value::Array(values) => pending.extend(values.iter_mut()),
            Value::Object(values) => pending.extend(values.values_mut()),
            _ => {}
        }
    }
}
struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl std::io::Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "WebSocket encoding exceeds payload limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_json_preserves_strings_null_and_strict_wire_bounds() {
        let codec = PayloadCodec::<Value>::serde()
            .with_json_encoding()
            .with_max_bytes(32)
            .unwrap();
        for value in [
            serde_json::json!(""),
            serde_json::json!("123"),
            serde_json::json!("null"),
            serde_json::json!("水 🦀"),
            Value::Null,
        ] {
            let bytes = codec.encode(&value).unwrap();
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), value);
            assert_eq!(codec.decode(&bytes).unwrap(), value);
        }
        for invalid in [
            &b"raw text"[..],
            &b"\"\xff\""[..],
            &b""[..],
            &b"null trailing"[..],
        ] {
            assert!(codec.decode(invalid).is_err());
        }
        let exact = PayloadCodec::<String>::serde()
            .with_json_encoding()
            .with_max_bytes(4)
            .unwrap();
        assert_eq!(exact.encode(&"ab".into()).unwrap(), b"\"ab\"");
        assert!(exact.encode(&"abc".into()).is_err());
        assert!(exact.decode(b"\"abc\"").is_err());
        assert_eq!(
            PayloadCodec::<String>::serde()
                .encode(&"123".into())
                .unwrap(),
            b"123"
        );
        assert!(PayloadCodec::<Value>::serde().encode(&Value::Null).is_err());
    }

    #[derive(Debug, PartialEq, Serialize, serde::Deserialize)]
    #[serde(tag = "type")]
    enum AppMessage {
        #[serde(rename = "chat")]
        Chat { text: String },
        #[serde(rename = "subscribe")]
        Subscribe { topic: String },
    }
    fn messages() -> PayloadCodec<AppMessage> {
        PayloadCodec::serde().with_validator(|value| {
            let text = match value {
                AppMessage::Chat { text } => text,
                AppMessage::Subscribe { topic } => topic,
            };
            if text.is_empty() || text.chars().count() > 32 {
                Err("message must contain 1–32 characters".into())
            } else {
                Ok(())
            }
        })
    }
    fn case<T: Serialize + DeserializeOwned>(codec: &PayloadCodec<T>, input: &Value) -> Value {
        if input["action"] == "encode" {
            let result = serde_json::from_value(input["value"].clone())
                .map_err(|error| PayloadError::Invalid(error.to_string()))
                .and_then(|value| codec.encode(&value));
            match result {
                Ok(bytes) => serde_json::json!({"ok":true,"bytes":bytes}),
                Err(_) => serde_json::json!({"ok":false}),
            }
        } else {
            let bytes: Vec<u8> = serde_json::from_value(input["bytes"].clone()).unwrap();
            match codec.decode(&bytes) {
                Ok(value) => serde_json::json!({"ok":true,"value":value}),
                Err(_) => serde_json::json!({"ok":false}),
            }
        }
    }

    #[cfg(feature = "ws-validation")]
    #[test]
    fn framework_codec_enforces_garde_on_incoming_and_outgoing_payloads() {
        #[derive(Serialize, serde::Deserialize, schemars::JsonSchema, garde::Validate)]
        struct Input {
            #[garde(length(min = 1, max = 8))]
            message: String,
        }
        let codec = PayloadCodec::<Input>::framework();
        assert!(codec.decode(br#"{"message":"hello"}"#).is_ok());
        assert!(codec.decode(br#"{"message":""}"#).is_err());
        assert!(codec.decode(br#"{"message":"too long string"}"#).is_err());
        assert!(
            codec
                .encode(&Input {
                    message: String::new()
                })
                .is_err()
        );
        assert_eq!(
            codec
                .encode(&Input {
                    message: "hello".into()
                })
                .unwrap(),
            br#"{"message":"hello"}"#
        );
    }

    #[test]
    fn json_representation_limits_do_not_bypass_schemas_as_plain_text() {
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(
            PayloadCodec::<String>::serde()
                .decode(deep.as_bytes())
                .is_err()
        );
        assert!(
            PayloadCodec::<Value>::serde()
                .decode(deep.as_bytes())
                .is_err()
        );
        assert!(PayloadCodec::<String>::serde().decode(b"1e400").is_err());
        assert_eq!(
            PayloadCodec::<String>::serde().decode(b"{broken").unwrap(),
            "{broken"
        );
    }

    #[test]
    fn native_ws_payload_conformance() {
        let Ok(input) = std::env::var("FORKLAUNCH_TEST_WS_PAYLOAD_INPUT") else {
            return;
        };
        let cases: Vec<Value> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let output = cases
            .iter()
            .map(|input| match input["kind"].as_str().unwrap() {
                "text" => case(&PayloadCodec::<String>::serde(), input),
                "value" => case(&PayloadCodec::<Value>::serde(), input),
                "message" => case(&messages(), input),
                "count" => case(
                    &PayloadCodec::<u64>::serde().with_validator(|count| {
                        if *count <= 1000 {
                            Ok(())
                        } else {
                            Err("count exceeds limit".into())
                        }
                    }),
                    input,
                ),
                _ => panic!("unknown payload fixture kind"),
            })
            .collect::<Vec<_>>();
        std::fs::write(
            std::env::var("FORKLAUNCH_TEST_WS_PAYLOAD_OUTPUT").unwrap(),
            serde_json::to_vec(&output).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn payload_types_validation_and_byte_limits_apply_before_delivery() {
        let codec = messages();
        let valid = br#"{"type":"chat","text":"hello","discarded":true}"#;
        assert_eq!(
            codec.decode(valid).unwrap(),
            AppMessage::Chat {
                text: "hello".into()
            }
        );
        for invalid in [
            br#"{"type":"chat","text":""}"#.as_slice(),
            br#"{"type":"other","text":"hello"}"#,
            br#"{"type":"chat","text":12}"#,
            b"null",
            b"not JSON",
        ] {
            assert!(codec.decode(invalid).is_err());
        }
        assert!(
            codec
                .encode(&AppMessage::Chat {
                    text: String::new()
                })
                .is_err()
        );
        let text = PayloadCodec::<String>::serde().with_max_bytes(3).unwrap();
        assert_eq!(text.encode(&"水".into()).unwrap(), "水".as_bytes());
        assert_eq!(text.encode(&"\0\0\0".into()).unwrap(), [0, 0, 0]);
        assert!(text.encode(&"水水".into()).is_err());
        assert!(text.decode(b"1234").is_err());
        assert!(PayloadCodec::<Value>::serde().encode(&Value::Null).is_err());
        assert_eq!(
            PayloadCodec::<Value>::serde().decode(b"null").unwrap(),
            Value::Null
        );
        assert!(PayloadCodec::<String>::serde().with_max_bytes(0).is_err());
        let bytes = PayloadCodec::bytes().with_max_bytes(3).unwrap();
        assert_eq!(bytes.decode(&[0, 255, 1]).unwrap(), [0, 255, 1]);
        assert_eq!(bytes.encode(&vec![0, 255, 1]).unwrap(), [0, 255, 1]);
        assert!(bytes.encode(&vec![0; 4]).is_err());
    }
}
