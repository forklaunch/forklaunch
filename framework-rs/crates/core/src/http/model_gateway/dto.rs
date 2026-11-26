//! Opt-in JSON DTOs; the opaque client does not validate these shapes.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// A JSON property which may be omitted. `Present(None)` preserves explicit null
/// for nullable fields, and `Present(Value::Null)` does so for unknown JSON fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum GatewayPresence<T> {
    #[default]
    Missing,
    Present(T),
}
impl<T> GatewayPresence<T> {
    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}
impl<T: Serialize> Serialize for GatewayPresence<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Missing => Err(serde::ser::Error::custom(
                "missing gateway property must be omitted",
            )),
            Self::Present(value) => value.serialize(serializer),
        }
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for GatewayPresence<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Present)
    }
}
// Serde serialize_with requires a borrowed field.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn finite<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("gateway number must be finite"));
    }
    value.serialize(serializer)
}
fn optional_finite<S: Serializer>(
    value: &GatewayPresence<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        GatewayPresence::Present(value) => finite(value, serializer),
        GatewayPresence::Missing => Err(serde::ser::Error::custom(
            "missing gateway property must be omitted",
        )),
    }
}
fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}
// Flattened extras must never override even an omitted known property. Deserialize
// consumes known fields first; this guard also covers callers mutating extras.
macro_rules! extras_serializer {
    ($name:ident, [$($reserved:literal),*]) => {
        fn $name<S: Serializer>(extras: &Map<String, Value>, serializer: S) -> Result<S::Ok, S::Error> {
            if extras.keys().any(|key| matches!(key.as_str(), $($reserved)|*)) {
                return Err(serde::ser::Error::custom("gateway extra collides with a declared property"));
            }
            extras.serialize(serializer)
        }
    };
}
/// Roles accepted by the TypeScript request interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GatewayChatRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatMessage {
    pub role: GatewayChatRole,
    #[serde(deserialize_with = "Value::deserialize")]
    pub content: Value,
    #[serde(flatten, serialize_with = "extras_gatewaychatmessage")]
    pub extra: Map<String, Value>,
}
extras_serializer!(extras_gatewaychatmessage, ["role", "content"]);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatRequest {
    pub model: String,
    pub messages: Vec<GatewayChatMessage>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub temperature: GatewayPresence<f64>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub top_p: GatewayPresence<f64>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub max_tokens: GatewayPresence<f64>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub max_completion_tokens: GatewayPresence<f64>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub response_format: GatewayPresence<Value>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub tools: GatewayPresence<Vec<Value>>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub tool_choice: GatewayPresence<Value>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub parallel_tool_calls: GatewayPresence<bool>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub stop: GatewayPresence<Value>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub seed: GatewayPresence<f64>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub presence_penalty: GatewayPresence<f64>,
    #[serde(
        default,
        skip_serializing_if = "GatewayPresence::is_missing",
        serialize_with = "optional_finite"
    )]
    pub frequency_penalty: GatewayPresence<f64>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub reasoning_effort: GatewayPresence<String>,
    #[serde(flatten, serialize_with = "extras_gatewaychatrequest")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaychatrequest,
    [
        "model",
        "messages",
        "temperature",
        "top_p",
        "max_tokens",
        "max_completion_tokens",
        "response_format",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "stop",
        "seed",
        "presence_penalty",
        "frequency_penalty",
        "reasoning_effort"
    ]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatCompletion {
    pub id: String,
    pub object: String,
    #[serde(serialize_with = "finite")]
    pub created: f64,
    pub model: String,
    pub choices: Vec<GatewayChatCompletionChoice>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub usage: GatewayPresence<GatewayUsage>,
    #[serde(flatten, serialize_with = "extras_gatewaychatcompletion")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaychatcompletion,
    ["id", "object", "created", "model", "choices", "usage"]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatCompletionChoice {
    #[serde(serialize_with = "finite")]
    pub index: f64,
    pub message: GatewayCompletionMessage,
    #[serde(deserialize_with = "required_nullable")]
    pub finish_reason: Option<String>,
    #[serde(flatten, serialize_with = "extras_gatewaychatcompletionchoice")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaychatcompletionchoice,
    ["index", "message", "finish_reason"]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayCompletionMessage {
    pub role: String,
    #[serde(deserialize_with = "required_nullable")]
    pub content: Option<String>,
    #[serde(flatten, serialize_with = "extras_gatewaycompletionmessage")]
    pub extra: Map<String, Value>,
}
extras_serializer!(extras_gatewaycompletionmessage, ["role", "content"]);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayUsage {
    #[serde(serialize_with = "finite")]
    pub prompt_tokens: f64,
    #[serde(serialize_with = "finite")]
    pub completion_tokens: f64,
    #[serde(serialize_with = "finite")]
    pub total_tokens: f64,
    #[serde(flatten, serialize_with = "extras_gatewayusage")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewayusage,
    ["prompt_tokens", "completion_tokens", "total_tokens"]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatChunk {
    pub id: String,
    pub object: String,
    pub model: String,
    pub choices: Vec<GatewayChatChunkChoice>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub usage: GatewayPresence<GatewayUsage>,
    #[serde(flatten, serialize_with = "extras_gatewaychatchunk")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaychatchunk,
    ["id", "object", "model", "choices", "usage"]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatChunkChoice {
    #[serde(serialize_with = "finite")]
    pub index: f64,
    pub delta: GatewayChatDelta,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub finish_reason: GatewayPresence<Option<String>>,
    #[serde(flatten, serialize_with = "extras_gatewaychatchunkchoice")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaychatchunkchoice,
    ["index", "delta", "finish_reason"]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayChatDelta {
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub role: GatewayPresence<String>,
    #[serde(default, skip_serializing_if = "GatewayPresence::is_missing")]
    pub content: GatewayPresence<Option<String>>,
    #[serde(flatten, serialize_with = "extras_gatewaychatdelta")]
    pub extra: Map<String, Value>,
}
extras_serializer!(extras_gatewaychatdelta, ["role", "content"]);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayModels {
    pub models: Vec<GatewayModel>,
    #[serde(rename = "monthlyTokenBudget", serialize_with = "finite")]
    pub monthly_token_budget: f64,
    #[serde(rename = "tokensUsedThisMonth", serialize_with = "finite")]
    pub tokens_used_this_month: f64,
    #[serde(rename = "requestsPerMinute", serialize_with = "finite")]
    pub requests_per_minute: f64,
    #[serde(
        rename = "withheldForHipaa",
        default,
        skip_serializing_if = "GatewayPresence::is_missing"
    )]
    pub withheld_for_hipaa: GatewayPresence<Vec<String>>,
    #[serde(flatten, serialize_with = "extras_gatewaymodels")]
    pub extra: Map<String, Value>,
}
extras_serializer!(
    extras_gatewaymodels,
    [
        "models",
        "monthlyTokenBudget",
        "tokensUsedThisMonth",
        "requestsPerMinute",
        "withheldForHipaa"
    ]
);

/// Opt-in managed gateway JSON shape; unknown properties are retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GatewayModel {
    pub id: String,
    pub baa: bool,
    #[serde(flatten, serialize_with = "extras_gatewaymodel")]
    pub extra: Map<String, Value>,
}
extras_serializer!(extras_gatewaymodel, ["id", "baa"]);

#[cfg(test)]
mod tests;
