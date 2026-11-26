#![forbid(unsafe_code)]

#[cfg(feature = "typed-schema")]
pub mod bindings;
#[cfg(feature = "typed-schema")]
mod typed;
#[cfg(feature = "typed-schema")]
pub use forklaunch_core_macros::socket_api;
#[cfg(feature = "typed-schema")]
pub use typed::TypedSocket;

mod artifact;
pub use artifact::SocketContractArtifact;
pub mod codec;
#[cfg(feature = "schema-validation")]
mod codec_schema;
pub mod contract;
pub use contract::{AsyncApiEnrichment, SocketContract, SocketSchemaEntry};

use crate::contract::SchemaRef;
use forklaunch_common::json_string;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    ClientToServer,
    ServerToClient,
}

/// The kinds of frames exchanged over a `ForkLaunch` WebSocket channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsMessageKind {
    /// An application message sent by the client.
    Client,
    /// An application message sent by the server.
    Server,
    /// A keepalive request.
    Ping,
    /// A keepalive response.
    Pong,
    /// A structured error frame.
    Error,
    /// A close frame carrying a reason.
    Close,
}

impl WsMessageKind {
    /// Returns the reserved control event name for keepalive/control frames.
    #[must_use]
    pub const fn control_event(self) -> Option<&'static str> {
        match self {
            Self::Ping => Some("ping"),
            Self::Pong => Some("pong"),
            Self::Error => Some("error"),
            Self::Close => Some("close"),
            Self::Client | Self::Server => None,
        }
    }
}

/// Why an inbound or outbound WebSocket frame was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsValidationError {
    /// The event is not declared on the channel.
    UnknownEvent(String),
    /// The event exists but is not allowed in this direction.
    WrongDirection {
        /// The offending event name.
        event: String,
        /// The direction the event is actually declared for.
        declared: MessageDirection,
    },
    /// The frame was not valid JSON or was missing the `event` field.
    MalformedFrame(String),
}

impl std::fmt::Display for WsValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEvent(event) => write!(formatter, "unknown event `{event}`"),
            Self::WrongDirection { event, declared } => write!(
                formatter,
                "event `{event}` is only valid for {}",
                declared.as_str()
            ),
            Self::MalformedFrame(reason) => write!(formatter, "malformed frame: {reason}"),
        }
    }
}

impl std::error::Error for WsValidationError {}

impl MessageDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientToServer => "clientToServer",
            Self::ServerToClient => "serverToClient",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContract {
    pub name: String,
    pub direction: MessageDirection,
    pub schema_name: String,
}

impl EventContract {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        direction: MessageDirection,
        schema_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            direction,
            schema_name: schema_name.into(),
        }
    }

    #[must_use]
    pub fn schema_ref(&self) -> SchemaRef {
        SchemaRef::named(self.schema_name.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketContract {
    pub path: String,
    pub events: Vec<EventContract>,
    pub error_schema: Option<String>,
    pub close_schema: Option<String>,
}

impl WebSocketContract {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: normalize_ws_path(&path.into()),
            events: Vec::new(),
            error_schema: None,
            close_schema: None,
        }
    }

    #[must_use]
    pub fn with_event(mut self, event: EventContract) -> Self {
        self.events.push(event);
        self
    }

    /// Declares the payload schema used for `error` control frames.
    #[must_use]
    pub fn with_error_schema(mut self, schema_name: impl Into<String>) -> Self {
        self.error_schema = Some(schema_name.into());
        self
    }

    /// Declares the payload schema used for `close` control frames.
    #[must_use]
    pub fn with_close_schema(mut self, schema_name: impl Into<String>) -> Self {
        self.close_schema = Some(schema_name.into());
        self
    }

    #[must_use]
    pub fn events_for_direction(&self, direction: MessageDirection) -> Vec<&EventContract> {
        self.events
            .iter()
            .filter(|event| event.direction == direction)
            .collect()
    }

    /// Validates a frame the server received from a client (client-to-server).
    ///
    /// # Errors
    ///
    /// Returns [`WsValidationError`] when the event is unknown or declared only
    /// for the opposite direction. Reserved control events always pass.
    pub fn validate_incoming(&self, event: &str) -> Result<(), WsValidationError> {
        self.validate_event(event, MessageDirection::ClientToServer)
    }

    /// Validates a frame the server is about to send (server-to-client).
    ///
    /// # Errors
    ///
    /// Returns [`WsValidationError`] when the event is unknown or declared only
    /// for the opposite direction. Reserved control events always pass.
    pub fn validate_outgoing(&self, event: &str) -> Result<(), WsValidationError> {
        self.validate_event(event, MessageDirection::ServerToClient)
    }

    fn validate_event(
        &self,
        event: &str,
        direction: MessageDirection,
    ) -> Result<(), WsValidationError> {
        if is_control_event(event) {
            return Ok(());
        }

        match self.events.iter().find(|declared| declared.name == event) {
            Some(declared) if declared.direction == direction => Ok(()),
            Some(declared) => Err(WsValidationError::WrongDirection {
                event: event.to_owned(),
                declared: declared.direction,
            }),
            None => Err(WsValidationError::UnknownEvent(event.to_owned())),
        }
    }
}

fn is_control_event(event: &str) -> bool {
    matches!(event, "ping" | "pong" | "error" | "close")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketEnvelope {
    pub event: String,
    pub payload_json: String,
    pub correlation_id: Option<String>,
}

impl WebSocketEnvelope {
    #[must_use]
    pub fn new(event: impl Into<String>, payload_json: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            payload_json: payload_json.into(),
            correlation_id: None,
        }
    }

    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Encodes the envelope into a JSON wire frame.
    #[must_use]
    pub fn to_json(&self) -> String {
        let payload = if self.payload_json.trim().is_empty() {
            "null"
        } else {
            self.payload_json.as_str()
        };

        let mut fields = vec![
            format!("\"event\":{}", json_string(&self.event)),
            format!("\"payload\":{payload}"),
        ];
        if let Some(correlation_id) = &self.correlation_id {
            fields.push(format!("\"correlationId\":{}", json_string(correlation_id)));
        }
        format!("{{{}}}", fields.join(","))
    }

    /// Decodes a JSON wire frame into an envelope.
    ///
    /// # Errors
    ///
    /// Returns [`WsValidationError::MalformedFrame`] when the input is not valid
    /// JSON or is missing a string `event` field.
    pub fn from_json(raw: &str) -> Result<Self, WsValidationError> {
        let value: serde_json::Value = serde_json::from_str(raw)
            .map_err(|error| WsValidationError::MalformedFrame(error.to_string()))?;

        let event = value
            .get("event")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| WsValidationError::MalformedFrame("missing `event` field".to_owned()))?
            .to_owned();

        let payload_json = value
            .get("payload")
            .map_or_else(|| "null".to_owned(), ToString::to_string);

        let correlation_id = value
            .get("correlationId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);

        Ok(Self {
            event,
            payload_json,
            correlation_id,
        })
    }
}

fn normalize_ws_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with('/') {
        trimmed.trim_end_matches('/').to_owned()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_contract_groups_events_by_direction() {
        let contract = WebSocketContract::new("realtime")
            .with_event(EventContract::new(
                "subscribe",
                MessageDirection::ClientToServer,
                "SubscribeRequest",
            ))
            .with_event(EventContract::new(
                "user.updated",
                MessageDirection::ServerToClient,
                "UserUpdatedEvent",
            ));

        assert_eq!(contract.path, "/realtime");
        assert_eq!(
            contract
                .events_for_direction(MessageDirection::ServerToClient)
                .len(),
            1
        );
    }

    fn sample_contract() -> WebSocketContract {
        WebSocketContract::new("realtime")
            .with_event(EventContract::new(
                "subscribe",
                MessageDirection::ClientToServer,
                "SubscribeRequest",
            ))
            .with_event(EventContract::new(
                "user.updated",
                MessageDirection::ServerToClient,
                "UserUpdatedEvent",
            ))
            .with_error_schema("WsError")
            .with_close_schema("WsClose")
    }

    #[test]
    fn validates_message_direction() {
        let contract = sample_contract();

        assert!(contract.validate_incoming("subscribe").is_ok());
        assert!(contract.validate_outgoing("user.updated").is_ok());
        // control frames always pass in either direction
        assert!(contract.validate_incoming("ping").is_ok());
        assert!(contract.validate_outgoing("close").is_ok());

        assert_eq!(
            contract.validate_incoming("user.updated"),
            Err(WsValidationError::WrongDirection {
                event: "user.updated".to_owned(),
                declared: MessageDirection::ServerToClient,
            })
        );
        assert_eq!(
            contract.validate_incoming("unknown"),
            Err(WsValidationError::UnknownEvent("unknown".to_owned()))
        );
    }

    #[test]
    fn envelope_round_trips_through_json() {
        let envelope =
            WebSocketEnvelope::new("user.updated", r#"{"id":"42"}"#).with_correlation_id("corr-1");
        let encoded = envelope.to_json();
        assert_eq!(
            encoded,
            r#"{"event":"user.updated","payload":{"id":"42"},"correlationId":"corr-1"}"#
        );

        let decoded = WebSocketEnvelope::from_json(&encoded).expect("frame should decode");
        assert_eq!(decoded.event, "user.updated");
        assert_eq!(decoded.correlation_id.as_deref(), Some("corr-1"));
        assert!(decoded.payload_json.contains("\"id\":\"42\""));

        assert!(matches!(
            WebSocketEnvelope::from_json("not json"),
            Err(WsValidationError::MalformedFrame(_))
        ));
        assert!(matches!(
            WebSocketEnvelope::from_json(r#"{"payload":1}"#),
            Err(WsValidationError::MalformedFrame(_))
        ));
    }

    #[test]
    fn control_event_names_are_reserved() {
        assert_eq!(WsMessageKind::Ping.control_event(), Some("ping"));
        assert_eq!(WsMessageKind::Client.control_event(), None);
    }
}
