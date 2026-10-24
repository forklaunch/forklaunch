//! Fallible JSON serialization with optional tenant-bound, TypeScript-compatible envelopes.
use super::{
    TenantContext,
    encryption::{FieldEncryptor, is_encrypted_ciphertext},
};
use crate::common::{FrameworkError, FrameworkResult};
use serde::{Serialize, de::DeserializeOwned};

/// Holds keys, but deliberately exposes neither them nor a `Debug` implementation.
pub struct JsonRecordCodec {
    encryptor: FieldEncryptor,
    typescript_safe_values: bool,
}

impl JsonRecordCodec {
    #[must_use]
    pub fn new(encryptor: FieldEncryptor) -> Self {
        Self {
            encryptor,
            typescript_safe_values: false,
        }
    }

    /// Enable TypeScript `safeStringify`/`safeParse` behavior for JSON-compatible
    /// values: top-level strings are written verbatim, and non-JSON plaintext is
    /// read as a string. Strings containing valid JSON parse into that JSON value.
    /// This does not emulate JavaScript classes, `BigInt` or number rounding.
    /// Authentication errors still fail closed before parsing; strict JSON remains
    /// the default. Typed callers can reject the resulting value during decoding.
    #[must_use]
    pub const fn with_typescript_safe_values(mut self) -> Self {
        self.typescript_safe_values = true;
        self
    }

    /// # Errors
    /// Rejects absent/invalid encryption configuration.
    pub fn from_env() -> FrameworkResult<Self> {
        Ok(Self::new(FieldEncryptor::from_env()?))
    }

    /// Encrypts with a fresh random nonce when tenant context is supplied; otherwise writes ordinary JSON.
    /// Cache/queue/object values do not require deterministic equality ciphertext.
    /// Writes use the TypeScript-readable v4 field envelope; authenticated legacy readers remain.
    /// # Errors
    /// Returns a safe error on serialization or encryption failure.
    pub fn encode<T: Serialize + ?Sized>(
        &self,
        value: &T,
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Vec<u8>> {
        let mut json = serde_json::to_string(value).map_err(|_| json_error())?;
        if self.typescript_safe_values && json.starts_with('"') {
            json = serde_json::from_str::<String>(&json).map_err(|_| json_error())?;
        }
        match tenant {
            Some(tenant) => self
                .encryptor
                .seal(&json, tenant.id(), "")
                .map(String::into_bytes),
            None => Ok(json.into_bytes()),
        }
    }

    /// Reads legacy plaintext JSON alongside authenticated envelopes.
    /// Encrypted records require tenant context, and failures never become cache misses.
    /// # Errors
    /// Rejects invalid UTF-8, missing context, wrong tenants/keys and tampered
    /// envelopes. Strict mode also rejects invalid JSON; either mode rejects
    /// values incompatible with the requested Rust type.
    pub fn decode<T: DeserializeOwned>(
        &self,
        bytes: &[u8],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<T> {
        let text = std::str::from_utf8(bytes).map_err(|_| json_error())?;
        let plaintext;
        let json = if is_encrypted_ciphertext(text) {
            let tenant = tenant.ok_or_else(|| {
                FrameworkError::new(
                    "record_tenant_required",
                    "Encrypted record requires tenant context",
                )
            })?;
            plaintext = self.encryptor.decrypt(text, tenant.id())?;
            &plaintext
        } else {
            text
        };
        if self.typescript_safe_values {
            let value = serde_json::from_str::<serde_json::Value>(json)
                .unwrap_or_else(|_| serde_json::Value::String(json.to_owned()));
            serde_json::from_value(value).map_err(|_| json_error())
        } else {
            serde_json::from_str(json).map_err(|_| json_error())
        }
    }

    /// Decode and apply JavaScript truthiness before converting to a Rust type.
    /// Null, false, numeric zero and empty strings are omitted; empty arrays and
    /// objects remain present. Enable `with_typescript_safe_values` separately
    /// when interoperating with TypeScript's top-level string wire encoding.
    ///
    /// # Errors
    /// Authentication and parsing failures propagate before filtering. Retained
    /// values must deserialize as the requested Rust type.
    pub fn decode_typescript_truthy<T: DeserializeOwned>(
        &self,
        bytes: &[u8],
        tenant: Option<&TenantContext>,
    ) -> FrameworkResult<Option<T>> {
        let value = self.decode::<serde_json::Value>(bytes, tenant)?;
        let truthy = match &value {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(value) => *value,
            serde_json::Value::Number(value) => value.as_f64() != Some(0.0),
            serde_json::Value::String(value) => !value.is_empty(),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
        };
        if truthy {
            serde_json::from_value(value)
                .map(Some)
                .map_err(|_| json_error())
        } else {
            Ok(None)
        }
    }
}

fn json_error() -> FrameworkError {
    FrameworkError::new(
        "record_json_invalid",
        "Unable to encode or decode JSON record",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn encrypted_records_randomize_writes_and_read_legacy_after_rotation() {
        let tenant = TenantContext::new("tenant-a").unwrap();
        let old = FieldEncryptor::new("old-record-key-fixture").unwrap();
        let legacy = old.encrypt("{\"private\":true}", tenant.id()).unwrap();
        let codec = JsonRecordCodec::new(
            FieldEncryptor::new("new-record-key-fixture")
                .unwrap()
                .with_previous_keys(&["old-record-key-fixture".to_owned()])
                .unwrap(),
        );
        let value = json!({"private": true});
        let first = codec.encode(&value, Some(&tenant)).unwrap();
        let second = codec.encode(&value, Some(&tenant)).unwrap();
        assert!(first.starts_with(b"v4:"));
        assert_ne!(first, second);
        for bytes in [&first[..], &second[..], legacy.as_bytes()] {
            assert_eq!(codec.decode::<Value>(bytes, Some(&tenant)).unwrap(), value);
            assert!(
                codec
                    .decode::<Value>(bytes, Some(&TenantContext::new("tenant-b").unwrap()))
                    .is_err()
            );
        }
    }

    #[test]
    fn safe_value_fallback_does_not_hide_type_or_authentication_failures() {
        let codec = JsonRecordCodec::new(FieldEncryptor::new("safe-value-fixture").unwrap())
            .with_typescript_safe_values();
        assert_eq!(codec.encode("plain", None).unwrap(), b"plain");
        assert_eq!(
            codec.decode::<String>(b"{invalid", None).unwrap(),
            "{invalid"
        );
        assert_eq!(codec.decode::<Value>(b"false", None).unwrap(), json!(false));
        assert!(codec.decode::<String>(b"false", None).is_err());
        assert!(codec.decode::<bool>(b"plain", None).is_err());
        assert!(codec.decode::<Value>(b"\xff", None).is_err());
        let tenant = TenantContext::new("tenant-a").unwrap();
        let bytes = codec.encode("plain", Some(&tenant)).unwrap();
        assert_eq!(
            codec.decode::<String>(&bytes, Some(&tenant)).unwrap(),
            "plain"
        );
        assert_eq!(
            codec.decode::<Value>(&bytes, None).unwrap_err().code(),
            "record_tenant_required"
        );
        assert!(
            codec
                .decode::<Value>(&bytes, Some(&TenantContext::new("tenant-b").unwrap()))
                .is_err()
        );
        assert!(
            codec
                .decode::<Value>(b"v2:bad:bad:bad", Some(&tenant))
                .is_err()
        );
    }

    #[test]
    fn literal_serde_marker_keys_survive_plain_and_encrypted_records() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Record {
            metadata: Value,
        }
        let codec = JsonRecordCodec::new(FieldEncryptor::new("literal-fixture-key").unwrap());
        let tenant = TenantContext::new("tenant-a").unwrap();
        // A first key of `$serde_json::private::RawValue` is upstream serde_json's raw-value marker; it is not object data.
        for key in ["$serde_json::private::Number"] {
            for value in [json!("123"), json!({"nested":true}), json!(null)] {
                let literal = Value::Object([(key.to_owned(), value)].into_iter().collect());
                for context in [None, Some(&tenant)] {
                    let bytes = codec.encode(&literal, context).unwrap();
                    assert_eq!(codec.decode::<Value>(&bytes, context).unwrap(), literal);
                    let record = Record {
                        metadata: literal.clone(),
                    };
                    let bytes = codec.encode(&record, context).unwrap();
                    assert_eq!(codec.decode::<Record>(&bytes, context).unwrap(), record);
                }
            }
        }
    }

    #[test]
    fn authenticated_records_require_context_and_support_rotation_and_plaintext_migration() {
        let old = JsonRecordCodec::new(FieldEncryptor::new("old-record-key").unwrap());
        let codec = JsonRecordCodec::new(
            FieldEncryptor::new("new-record-key")
                .unwrap()
                .with_previous_keys(&["old-record-key".into()])
                .unwrap(),
        );
        let tenant = TenantContext::new("tenant-a").unwrap();
        let wrong = TenantContext::new("tenant-b").unwrap();
        for value in [
            json!({"secret":"秘密🙂","empty":null}),
            json!(false),
            json!(0),
            json!([]),
            json!("v2:ordinary-json-string"),
        ] {
            let bytes = old.encode(&value, Some(&tenant)).unwrap();
            assert_eq!(codec.decode::<Value>(&bytes, Some(&tenant)).unwrap(), value);
            assert!(codec.decode::<Value>(&bytes, None).is_err());
            assert!(codec.decode::<Value>(&bytes, Some(&wrong)).is_err());
            assert_eq!(
                codec
                    .decode::<Value>(&codec.encode(&value, None).unwrap(), Some(&tenant))
                    .unwrap(),
                value
            );
            let mut corrupt = bytes;
            corrupt.push(b'!');
            assert!(codec.decode::<Value>(&corrupt, Some(&tenant)).is_err());
        }
        for bad in [b"\xff".as_slice(), b"v2:bad:bad:bad", b"{", b"v9:unknown"] {
            assert!(codec.decode::<Value>(bad, Some(&tenant)).is_err());
        }
    }
}
