//! Typed JSON messages for existing native SSE and WebSocket lifecycles.
use super::EndpointError;
use forklaunch_core::contract::schema_validation::{CheckedJsonValidator, compile_checked};
use serde::{Serialize, de::DeserializeOwned};
use std::{marker::PhantomData, sync::Arc};

/// Validates both directions against the exact published message schema, including
/// constraints Rust's type checker cannot enforce. This is not encryption or an
/// authorization policy: authenticated lifecycle/tenant routing and TLS are separate.
pub struct JsonMessage<T> {
    validator: Arc<CheckedJsonValidator>,
    max_bytes: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> Clone for JsonMessage<T> {
    fn clone(&self) -> Self {
        Self {
            validator: self.validator.clone(),
            max_bytes: self.max_bytes,
            marker: PhantomData,
        }
    }
}
impl<T: forklaunch_core::contract::__private::schemars::JsonSchema> JsonMessage<T> {
    /// # Errors
    /// Rejects divergent/invalid published schemas and zero message limits.
    pub fn new(published: &serde_json::Value, max_bytes: usize) -> Result<Self, &'static str> {
        if max_bytes == 0 || *published != forklaunch_core::contract::schema_value_for::<T>() {
            return Err("message type differs from published contract");
        }
        Ok(Self {
            validator: compile_checked(published).map_err(|_| "invalid message schema")?,
            max_bytes,
            marker: PhantomData,
        })
    }
}
impl<T: DeserializeOwned> JsonMessage<T> {
    /// # Errors
    /// Rejects oversized, malformed, or schema-invalid messages without echoing them.
    pub fn decode(&self, bytes: &[u8]) -> Result<T, EndpointError> {
        if bytes.len() > self.max_bytes {
            return Err(EndpointError::PayloadTooLarge);
        }
        let value = serde_json::from_slice(bytes).map_err(|_| EndpointError::InvalidRequest)?;
        (self.validator)(&value).map_err(|_| EndpointError::InvalidRequest)?;
        serde_json::from_value(value).map_err(|_| EndpointError::InvalidRequest)
    }
}
impl<T: Serialize> JsonMessage<T> {
    /// # Errors
    /// Rejects oversized, unserializable, or schema-invalid output messages.
    pub fn encode(&self, value: &T) -> Result<String, EndpointError> {
        let value = serde_json::to_value(value).map_err(|_| EndpointError::InvalidResponse)?;
        (self.validator)(&value).map_err(|_| EndpointError::InvalidResponse)?;
        let json = serde_json::to_string(&value).map_err(|_| EndpointError::InvalidResponse)?;
        if json.len() > self.max_bytes {
            return Err(EndpointError::InvalidResponse);
        }
        Ok(json)
    }
    /// # Errors
    /// Rejects invalid SSE control fields or invalid JSON data.
    pub fn sse_event(
        &self,
        id: &str,
        event: &str,
        value: &T,
    ) -> Result<axum::response::sse::Event, EndpointError> {
        if id.contains(['\n', '\r', '\0']) || event.contains(['\n', '\r', '\0']) {
            return Err(EndpointError::InvalidResponse);
        }
        Ok(axum::response::sse::Event::default()
            .id(id)
            .event(event)
            .data(self.encode(value)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct Message {
        #[schemars(length(min = 3))]
        text: String,
    }
    #[test]
    fn published_message_types_and_constraints_bind_both_directions() {
        let schema = forklaunch_core::contract::schema_value_for::<Message>();
        assert!(JsonMessage::<Message>::new(&serde_json::json!({"type":"string"}), 100).is_err());
        let codec = JsonMessage::<Message>::new(&schema, 32).unwrap();
        assert!(codec.decode(br#"{"text":"x"}"#).is_err());
        assert!(codec.encode(&Message { text: "x".into() }).is_err());
        assert!(
            codec
                .encode(&Message {
                    text: "x".repeat(32)
                })
                .is_err()
        );
        let message = codec.decode(br#"{"text":"hello"}"#).unwrap();
        assert_eq!(codec.encode(&message).unwrap(), r#"{"text":"hello"}"#);
        assert!(codec.sse_event("1", "note", &message).is_ok());
        assert!(
            codec
                .sse_event("1\nevent: secret", "note", &message)
                .is_err()
        );
    }
}
