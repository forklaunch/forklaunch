//! Schema metadata for raw typed sockets, separate from tagged event envelopes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// TypeScript-compatible channel/operation enrichment. A singular value takes
/// precedence; otherwise channels fan out and the first operation names the message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncApiEnrichment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocketSchemaEntry {
    pub shape: Value,
    /// Explicit raw bytes; never inferred from a JSON Schema format annotation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    /// Encode strings and null as JSON; reject non-JSON incoming bytes.
    #[serde(
        default,
        rename = "jsonEncoding",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub json_encoding: bool,
    #[serde(flatten)]
    pub enrichment: AsyncApiEnrichment,
}
impl SocketSchemaEntry {
    #[must_use]
    pub fn new(shape: Value) -> Self {
        Self {
            shape,
            binary: false,
            json_encoding: false,
            enrichment: AsyncApiEnrichment::default(),
        }
    }
    /// Explicit JSON media encoding, including quoted strings and null.
    #[must_use]
    pub fn json(shape: Value) -> Self {
        Self {
            json_encoding: true,
            ..Self::new(shape)
        }
    }
    /// Raw bytes without JSON interpretation. Message byte limits still apply.
    #[must_use]
    pub fn binary() -> Self {
        Self {
            binary: true,
            ..Self::new(serde_json::json!({"type":"string","format":"binary"}))
        }
    }
    /// Reject contradictory schema assertions rather than silently discarding them.
    #[must_use]
    pub fn valid_binary_shape(&self) -> bool {
        !self.binary
            || (!self.json_encoding
                && self.shape == serde_json::json!({"type":"string","format":"binary"}))
    }
    /// Derive the same schema used by a framework-validated payload type.
    ///
    /// # Errors
    /// Returns malformed generated schema errors.
    #[cfg(feature = "ws-validation")]
    pub fn framework<T: forklaunch_validator::FrameworkSchema>() -> Result<Self, serde_json::Error>
    {
        serde_json::from_str(&T::json_schema_json()).map(Self::new)
    }
    #[must_use]
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.enrichment.channel = Some(channel.into());
        self
    }
    #[must_use]
    pub fn with_channels(mut self, channels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.enrichment.channels = channels.into_iter().map(Into::into).collect();
        self
    }
    #[must_use]
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.enrichment.operation = Some(operation.into());
        self
    }
    #[must_use]
    pub fn with_operations(
        mut self,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.enrichment.operations = operations.into_iter().map(Into::into).collect();
        self
    }
    /// Effective channel names follow TypeScript's singular/plural/default precedence.
    #[must_use]
    pub fn channel_names<'a>(&'a self, message_name: &'a str) -> Vec<&'a str> {
        match self
            .enrichment
            .channel
            .as_deref()
            .filter(|name| !name.is_empty())
        {
            Some(channel) => vec![channel],
            None if !self.enrichment.channels.is_empty() => self
                .enrichment
                .channels
                .iter()
                .map(String::as_str)
                .collect(),
            None => vec![message_name],
        }
    }
    #[must_use]
    pub fn message_name<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.enrichment
            .operation
            .as_deref()
            .or_else(|| self.enrichment.operations.first().map(String::as_str))
            .unwrap_or(fallback)
    }
}
/// Raw UTF-8/JSON socket schema collections matching TypeScript's event schemas.
/// Ping/pong/close metadata describe protocol controls, not application envelopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketContract {
    pub path: String,
    /// Optional native handshake authorization and typed request metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake: Option<Value>,
    #[serde(default)]
    pub client_messages: BTreeMap<String, SocketSchemaEntry>,
    #[serde(default)]
    pub server_messages: BTreeMap<String, SocketSchemaEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, SocketSchemaEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ping: Option<SocketSchemaEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pong: Option<SocketSchemaEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub close_reason: BTreeMap<String, SocketSchemaEntry>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user_data: BTreeMap<String, Value>,
}
impl SocketContract {
    #[must_use]
    pub fn new(path: impl AsRef<str>) -> Self {
        Self {
            path: super::normalize_ws_path(path.as_ref()),
            handshake: None,
            client_messages: BTreeMap::new(),
            server_messages: BTreeMap::new(),
            errors: BTreeMap::new(),
            ping: None,
            pong: None,
            close_reason: BTreeMap::new(),
            context: BTreeMap::new(),
            user_data: BTreeMap::new(),
        }
    }
    #[must_use]
    pub fn with_client_message(
        mut self,
        name: impl Into<String>,
        entry: SocketSchemaEntry,
    ) -> Self {
        self.client_messages.insert(name.into(), entry);
        self
    }
    #[must_use]
    pub fn with_server_message(
        mut self,
        name: impl Into<String>,
        entry: SocketSchemaEntry,
    ) -> Self {
        self.server_messages.insert(name.into(), entry);
        self
    }
    #[must_use]
    pub fn with_error(mut self, name: impl Into<String>, entry: SocketSchemaEntry) -> Self {
        self.errors.insert(name.into(), entry);
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[cfg(feature = "ws-validation")]
    #[test]
    fn framework_schema_entry_conformance() {
        #[derive(Serialize, Deserialize, schemars::JsonSchema, garde::Validate)]
        struct Input {
            #[garde(length(min = 1, max = 8))]
            #[schemars(length(min = 1, max = 8))]
            message: String,
        }
        let codec = crate::ws::codec::PayloadCodec::<Input>::framework();
        assert!(codec.decode(br#"{"message":"hello"}"#).is_ok());
        assert!(codec.decode(br#"{"message":""}"#).is_err());
        let entry = SocketSchemaEntry::framework::<Input>().unwrap();
        assert_eq!(entry.shape["properties"]["message"]["minLength"], 1);
        let contract = SocketContract::new("/chat").with_client_message("request", entry);
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_FRAMEWORK_SOCKET_CONTRACT") {
            std::fs::write(output, serde_json::to_vec(&contract).unwrap()).unwrap();
        }
    }

    #[test]
    fn metadata_serializes_like_typescript_and_preserves_precedence() {
        let entry = SocketSchemaEntry::new(json!({"type":"string"}))
            .with_channels(["first", "second"])
            .with_channel("preferred")
            .with_operations(["send", "other"])
            .with_operation("preferred-operation");
        assert_eq!(entry.channel_names("fallback"), ["preferred"]);
        assert_eq!(entry.message_name("fallback"), "preferred-operation");
        let contract = SocketContract::new("chat").with_client_message("request", entry);
        let value = serde_json::to_value(&contract).unwrap();
        assert_eq!(value["path"], "/chat");
        assert_eq!(value["clientMessages"]["request"]["channel"], "preferred");
        assert_eq!(
            value["clientMessages"]["request"]["shape"],
            json!({"type":"string"})
        );
        assert_eq!(
            serde_json::from_value::<SocketContract>(value).unwrap(),
            contract
        );
        let entry = SocketSchemaEntry::new(json!(true))
            .with_channel("")
            .with_channels(["first", "second"])
            .with_operations(["send", "other"]);
        assert_eq!(entry.channel_names("fallback"), ["first", "second"]);
        assert_eq!(entry.message_name("fallback"), "send");
        assert_eq!(
            SocketSchemaEntry::new(json!(true)).channel_names("fallback"),
            ["fallback"]
        );
    }
}
