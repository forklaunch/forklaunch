#![forbid(unsafe_code)]

#[cfg(feature = "asyncapi-import")]
mod import;
#[cfg(feature = "asyncapi-import")]
pub use import::{
    ImportedWebSocketAsyncApi, import_server_websocket_asyncapi,
    import_server_websocket_asyncapi_with_metadata,
};

mod expression;
mod raw;
pub use expression::{MessageSource, RuntimeExpression};
pub use raw::{
    SocketAsyncApiOptions, generate_raw_socket_asyncapi_json,
    generate_raw_socket_asyncapi_json_with_options,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use forklaunch_common::{
    FrameworkError, FrameworkResult, JsonField, json_array, json_object, json_string,
};
use forklaunch_core::contract::SchemaRef;
use forklaunch_core::ws::{MessageDirection as WsDirection, WebSocketContract};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncApiInfo {
    pub title: String,
    pub version: String,
}

impl AsyncApiInfo {
    #[must_use]
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageProtocol {
    Kafka,
    Redis,
    WebSocket,
    HttpWebhook,
}

impl MessageProtocol {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kafka => "kafka",
            Self::Redis => "redis",
            Self::WebSocket => "ws",
            Self::HttpWebhook => "http",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    Publish,
    Subscribe,
}

impl MessageDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Subscribe => "subscribe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageContract {
    pub name: String,
    pub title: String,
    pub payload: SchemaRef,
    pub headers: BTreeMap<String, SchemaRef>,
    pub correlation_id: Option<RuntimeExpression>,
}

impl MessageContract {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        title: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            payload: SchemaRef::named(payload),
            headers: BTreeMap::new(),
            correlation_id: None,
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, schema: impl Into<String>) -> Self {
        self.headers.insert(name.into(), SchemaRef::named(schema));
        self
    }

    /// Declare a checked message correlation location; consumers own matching
    /// and deadlines. The expression preserves the referenced value's type.
    #[must_use]
    pub fn with_correlation_id(mut self, location: RuntimeExpression) -> Self {
        self.correlation_id = Some(location);
        self
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        let headers = self
            .headers
            .iter()
            .map(|(name, schema)| {
                format!(
                    "{{\"name\":{},\"schema\":{}}}",
                    json_string(name),
                    schema.to_json()
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        let correlation = self
            .correlation_id
            .as_ref()
            .map_or_else(String::new, |expression| {
                format!(
                    ",\"correlationId\":{{\"location\":{}}}",
                    json_string(expression.location())
                )
            });
        format!(
            "{{\"name\":{},\"title\":{},\"payload\":{},\"headers\":[{}]{}}}",
            json_string(&self.name),
            json_string(&self.title),
            self.payload.to_json(),
            headers,
            correlation
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelContract {
    pub name: String,
    pub address: String,
    pub protocol: MessageProtocol,
    pub direction: MessageDirection,
    pub message: MessageContract,
}

impl ChannelContract {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        address: impl Into<String>,
        protocol: MessageProtocol,
        direction: MessageDirection,
        message: MessageContract,
    ) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
            protocol,
            direction,
            message,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":{},\"address\":{},\"protocol\":{},\"direction\":{},\"message\":{}}}",
            json_string(&self.name),
            json_string(&self.address),
            json_string(self.protocol.as_str()),
            json_string(self.direction.as_str()),
            self.message.to_json()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncApiDocument {
    pub info: AsyncApiInfo,
    pub channels: Vec<ChannelContract>,
}

impl AsyncApiDocument {
    #[must_use]
    pub fn new(info: AsyncApiInfo) -> Self {
        Self {
            info,
            channels: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_channel(mut self, channel: ChannelContract) -> Self {
        self.channels.push(channel);
        self
    }

    /// Serializes an `AsyncAPI` 3.0 document with grouped channels and operations.
    /// Named payloads without supplied schemas remain unconstrained and retain
    /// their name as a title. Supply `SchemaRef::with_json_schema` for validation.
    /// # Errors
    /// Rejects malformed schemas, conflicting channel/message definitions,
    /// and empty document/contract identifiers.
    pub fn to_json(&self) -> FrameworkResult<String> {
        if self.info.title.is_empty() || self.info.version.is_empty() {
            return Err(invalid("document title and version are required"));
        }
        let mut channels = serde_json::Map::new();
        let mut operations = serde_json::Map::new();
        let mut definitions = BTreeMap::new();
        for (index, contract) in self.channels.iter().enumerate() {
            if contract.name.is_empty()
                || contract.address.is_empty()
                || contract.message.name.is_empty()
            {
                return Err(invalid(
                    "channel name, address and message name are required",
                ));
            }
            let definition = (&contract.address, contract.protocol);
            if definitions
                .insert(&contract.name, definition)
                .is_some_and(|previous| previous != definition)
            {
                return Err(invalid(
                    "a channel identifier has conflicting addresses or protocols",
                ));
            }
            let channel = channels.entry(contract.name.clone()).or_insert_with(|| {
                json!({
                    "address": contract.address,
                    "x-forklaunch-protocol": contract.protocol.as_str(),
                    "messages": {},
                })
            });
            let messages = channel
                .get_mut("messages")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| invalid("channel messages must be an object"))?;
            let headers = contract
                .message
                .headers
                .iter()
                .map(|(name, schema)| Ok((name.clone(), schema_value(schema)?)))
                .collect::<FrameworkResult<serde_json::Map<String, Value>>>()?;
            let mut message = json!({
                "name": contract.message.name,
                "title": contract.message.title,
                "payload": schema_value(&contract.message.payload)?,
            });
            if !headers.is_empty() {
                message["headers"] = json!({"type": "object", "properties": headers});
            }
            if let Some(expression) = &contract.message.correlation_id {
                message["correlationId"] = json!({"location":expression.location()});
            }
            if messages
                .get(&contract.message.name)
                .is_some_and(|existing| existing != &message)
            {
                return Err(invalid(
                    "a channel message identifier has conflicting schemas or metadata",
                ));
            }
            messages.insert(contract.message.name.clone(), message);
            let channel_pointer = format!("#/channels/{}", pointer_segment(&contract.name));
            operations.insert(
                format!("operation{}", index + 1),
                json!({
                    "action": match contract.direction {
                        MessageDirection::Publish => "send",
                        MessageDirection::Subscribe => "receive",
                    },
                    "channel": {"$ref": channel_pointer},
                    "messages": [{"$ref": format!("{channel_pointer}/messages/{}",
                        pointer_segment(&contract.message.name))}],
                }),
            );
        }
        Ok(json!({
            "asyncapi": "3.0.0", "info": {"title": self.info.title, "version": self.info.version},
            "channels": channels, "operations": operations,
        })
        .to_string())
    }
}

fn invalid(message: &str) -> FrameworkError {
    FrameworkError::new("asyncapi_contract_invalid", message)
}

fn schema_value(schema: &SchemaRef) -> FrameworkResult<Value> {
    if let Some(encoded) = &schema.json_schema {
        let value: Value = serde_json::from_str(encoded)
            .map_err(|_| invalid("message schema is not valid JSON"))?;
        if !value.is_object() && !value.is_boolean() {
            return Err(invalid("message schema must be an object or boolean"));
        }
        return Ok(value);
    }
    Ok(match schema.name.as_str() {
        "String" | "string" => json!({"type": "string"}),
        "Boolean" | "boolean" => json!({"type": "boolean"}),
        "Integer" | "integer" => json!({"type": "integer"}),
        "Number" | "number" => json!({"type": "number"}),
        _ => json!({"title": schema.name}),
    })
}

fn pointer_segment(value: &str) -> String {
    // JSON Pointer escaping happens before URI-fragment encoding.
    let escaped = value.replace('~', "~0").replace('/', "~1");
    let mut fragment = String::new();
    for byte in escaped.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            fragment.push(char::from(byte));
        } else {
            let _ = write!(fragment, "%{byte:02X}");
        }
    }
    fragment
}

/// Generates a spec-compliant `AsyncAPI` 3.0 document from a WebSocket contract.
///
/// Each contract maps to a single channel whose events become channel messages;
/// every event becomes an operation whose `action` reflects its direction
/// (`clientToServer` → `receive`, `serverToClient` → `send`). Output is
/// deterministic (sorted keys) so it is suitable for snapshot review.
#[must_use]
pub fn generate_asyncapi_json(info: &AsyncApiInfo, contract: &WebSocketContract) -> String {
    let channel = channel_name_from_path(&contract.path);

    let channel_messages = json_object(contract.events.iter().map(|event| {
        JsonField::new(
            &event.name,
            json_object([JsonField::new(
                "$ref",
                json_string(&format!(
                    "#/components/messages/{}",
                    pointer_segment(&event.schema_name)
                )),
            )]),
        )
    }));

    let channels = json_object([JsonField::new(
        &channel,
        json_object([
            JsonField::new("address", json_string(&contract.path)),
            JsonField::new("messages", channel_messages),
        ]),
    )]);

    let operations = json_object(contract.events.iter().map(|event| {
        let action = match event.direction {
            WsDirection::ClientToServer => "receive",
            WsDirection::ServerToClient => "send",
        };
        JsonField::new(
            &event.name,
            json_object([
                JsonField::new("action", json_string(action)),
                JsonField::new(
                    "channel",
                    json_object([JsonField::new(
                        "$ref",
                        json_string(&format!("#/channels/{}", pointer_segment(&channel))),
                    )]),
                ),
                JsonField::new(
                    "messages",
                    json_array([json_object([JsonField::new(
                        "$ref",
                        json_string(&format!(
                            "#/channels/{}/messages/{}",
                            pointer_segment(&channel),
                            pointer_segment(&event.name)
                        )),
                    )])]),
                ),
            ]),
        )
    }));

    let components = websocket_components(contract);

    json_object([
        JsonField::new("asyncapi", json_string("3.0.0")),
        JsonField::new("channels", channels),
        JsonField::new("components", components),
        JsonField::new(
            "info",
            json_object([
                JsonField::new("title", json_string(&info.title)),
                JsonField::new("version", json_string(&info.version)),
            ]),
        ),
        JsonField::new("operations", operations),
    ])
}

fn websocket_components(contract: &WebSocketContract) -> String {
    let mut messages = BTreeMap::<String, String>::new();
    let mut schema_names = BTreeSet::<String>::new();
    for event in &contract.events {
        schema_names.insert(event.schema_name.clone());
        messages
            .entry(event.schema_name.clone())
            .or_insert_with(|| {
                json_object([
                    JsonField::new("name", json_string(&event.schema_name)),
                    JsonField::new(
                        "payload",
                        json_object([JsonField::new(
                            "$ref",
                            json_string(&format!(
                                "#/components/schemas/{}",
                                pointer_segment(&event.schema_name)
                            )),
                        )]),
                    ),
                ])
            });
    }

    json_object([
        JsonField::new(
            "messages",
            json_object(
                messages
                    .into_iter()
                    .map(|(name, message)| JsonField::new(name, message)),
            ),
        ),
        JsonField::new(
            "schemas",
            json_object(schema_names.into_iter().map(|name| {
                JsonField::new(
                    name,
                    json_object([JsonField::new("type", json_string("object"))]),
                )
            })),
        ),
    ])
}

/// Generates a typed `AsyncAPI` document for the actual JSON event envelopes.
/// Supplied payload schemas are retained in components; event-name constants
/// distinguish messages that share a payload type. Declared error/close schemas
/// become server-to-client control messages. Ping/pong transport frames are omitted.
/// # Errors
/// Rejects missing/malformed schemas, duplicate/reserved events, invalid component
/// names, and unresolved/external schema references. No schema is invented.
pub fn generate_asyncapi_json_with_schemas(
    info: &AsyncApiInfo,
    contract: &WebSocketContract,
    schemas: &BTreeMap<String, Value>,
) -> FrameworkResult<String> {
    use forklaunch_core::ws::EventContract;
    for (name, schema) in schemas {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(invalid(
                "schema component names must contain only letters, digits, dot, underscore or hyphen",
            ));
        }
        if !schema.is_object() && !schema.is_boolean() {
            return Err(invalid("message schema must be an object or boolean"));
        }
    }
    let mut events = contract.events.clone();
    if events
        .iter()
        .any(|event| matches!(event.name.as_str(), "ping" | "pong" | "error" | "close"))
    {
        return Err(invalid(
            "application events cannot use reserved control names",
        ));
    }
    for (name, schema) in [
        ("error", &contract.error_schema),
        ("close", &contract.close_schema),
    ] {
        if let Some(schema) = schema {
            events.push(EventContract::new(
                name,
                WsDirection::ServerToClient,
                schema,
            ));
        }
    }
    let mut document = AsyncApiDocument::new(info.clone());
    let mut names = BTreeSet::new();
    for event in events {
        if !names.insert(event.name.clone()) {
            return Err(invalid("event names must be unique within a channel"));
        }
        if !schemas.contains_key(&event.schema_name) {
            return Err(invalid("an event payload schema is missing"));
        }
        let mut message = MessageContract::new(&event.name, &event.name, &event.schema_name);
        message.payload.json_schema = Some(
            json!({
                "type": "object", "required": ["event", "payload"],
                "properties": {
                    "event": {"type": "string", "const": event.name},
                    "payload": {"$ref": format!("#/components/schemas/{}", event.schema_name)},
                },
            })
            .to_string(),
        );
        document = document.with_channel(ChannelContract::new(
            channel_name_from_path(&contract.path),
            &contract.path,
            MessageProtocol::WebSocket,
            match event.direction {
                WsDirection::ClientToServer => MessageDirection::Subscribe,
                WsDirection::ServerToClient => MessageDirection::Publish,
            },
            message,
        ));
    }
    let mut encoded: Value = serde_json::from_str(&document.to_json()?)
        .map_err(|_| invalid("unable to encode document"))?;
    encoded["components"] = json!({"schemas": schemas});
    validate_references(&encoded, &encoded["components"], 0)?;
    Ok(encoded.to_string())
}

fn validate_references(root: &Value, value: &Value, depth: usize) -> FrameworkResult<()> {
    if depth > 64 {
        return Err(invalid("schema nesting exceeds 64 levels"));
    }
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref") {
                let reference = reference
                    .as_str()
                    .ok_or_else(|| invalid("schema reference must be a string"))?;
                if !reference.starts_with("#/components/schemas/") {
                    return Err(invalid(
                        "only local component schema references are supported",
                    ));
                }
                if root.pointer(&reference[1..]).is_none() {
                    return Err(invalid("schema reference cannot be resolved"));
                }
            }
            for child in object.values() {
                validate_references(root, child, depth + 1)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                validate_references(root, child, depth + 1)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn channel_name_from_path(path: &str) -> String {
    let trimmed = path.trim_matches('/').replace('/', ".");
    if trimmed.is_empty() {
        "root".to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_correlation_exports_checked_locations_and_conflict_guards() {
        let message = MessageContract::new("Created", "Created", "String")
            .with_correlation_id(RuntimeExpression::parse("$message.payload#/id").unwrap());
        let contract: Value = serde_json::from_str(&message.to_json()).unwrap();
        assert_eq!(
            contract["correlationId"]["location"],
            "$message.payload#/id"
        );
        let channel = ChannelContract::new(
            "events",
            "events",
            MessageProtocol::Kafka,
            MessageDirection::Publish,
            message,
        );
        let document =
            AsyncApiDocument::new(AsyncApiInfo::new("Events", "1")).with_channel(channel.clone());
        let encoded: Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
        assert_eq!(
            encoded["channels"]["events"]["messages"]["Created"]["correlationId"]["location"],
            "$message.payload#/id"
        );
        let mut changed = channel;
        changed.message.correlation_id =
            Some(RuntimeExpression::parse("$message.header#/id").unwrap());
        assert!(document.with_channel(changed).to_json().is_err());
        let plain: Value =
            serde_json::from_str(&MessageContract::new("Plain", "Plain", "String").to_json())
                .unwrap();
        assert!(plain.get("correlationId").is_none());
        if let Ok(path) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_CORRELATION_EXPORT") {
            std::fs::write(path, serde_json::to_vec(&encoded).unwrap()).unwrap();
        }
    }

    #[test]
    fn asyncapi_document_serializes_channels() {
        let document = AsyncApiDocument::new(AsyncApiInfo::new("Orders", "1.0.0")).with_channel(
            ChannelContract::new(
                "orders-created",
                "orders.created",
                MessageProtocol::Kafka,
                MessageDirection::Subscribe,
                MessageContract::new("OrderCreated", "Order Created", "OrderCreatedPayload")
                    .with_header("correlationId", "String"),
            ),
        );

        let encoded = document.to_json().unwrap();

        assert!(encoded.contains("\"asyncapi\":\"3.0.0\""));
        assert!(encoded.contains("\"orders.created\""));
        assert!(encoded.contains("\"OrderCreatedPayload\""));
    }

    #[test]
    fn asyncapi_groups_channels_and_preserves_schemas_and_both_directions() {
        let mut message = MessageContract::new("item/~created", "Created\n\titem", "Item")
            .with_header("correlation", "String");
        message.payload = message.payload.with_json_schema(r#"{"type":"object","required":["id"],"properties":{"id":{"type":"integer"}},"additionalProperties":false}"#);
        let send = ChannelContract::new(
            "orders/~feed",
            "orders.feed",
            MessageProtocol::Kafka,
            MessageDirection::Publish,
            message.clone(),
        );
        let mut receive = send.clone();
        receive.direction = MessageDirection::Subscribe;
        let doc = AsyncApiDocument::new(AsyncApiInfo::new("Orders\nAPI", "1.0.0"))
            .with_channel(send)
            .with_channel(receive);
        let encoded = doc.to_json().unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["channels"].as_object().unwrap().len(), 1);
        assert_eq!(value["operations"]["operation1"]["action"], "send");
        assert_eq!(value["operations"]["operation2"]["action"], "receive");
        assert_eq!(
            value["operations"]["operation1"]["channel"]["$ref"],
            "#/channels/orders~1~0feed"
        );
        assert_eq!(
            value["operations"]["operation1"]["messages"][0]["$ref"],
            "#/channels/orders~1~0feed/messages/item~1~0created"
        );
        let payload = &value["channels"]["orders/~feed"]["messages"]["item/~created"];
        assert_eq!(payload["payload"]["properties"]["id"]["type"], "integer");
        assert_eq!(
            payload["headers"]["properties"]["correlation"]["type"],
            "string"
        );
        assert_eq!(payload["title"], "Created\n\titem");
        assert_eq!(doc.to_json().unwrap(), encoded);
        if let Ok(path) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_DOCUMENT") {
            std::fs::write(path, encoded).unwrap();
        }
    }

    #[test]
    fn asyncapi_rejects_ambiguous_contracts_and_malformed_schemas() {
        let channel = ChannelContract::new(
            "events",
            "events.created",
            MessageProtocol::Redis,
            MessageDirection::Subscribe,
            MessageContract::new("Created", "Created", "Payload"),
        );
        let document = AsyncApiDocument::new(AsyncApiInfo::new("Events", "1.0.0"))
            .with_channel(channel.clone());
        for changed in [
            ChannelContract {
                address: "other".into(),
                ..channel.clone()
            },
            ChannelContract {
                protocol: MessageProtocol::Kafka,
                ..channel.clone()
            },
            ChannelContract {
                name: String::new(),
                ..channel.clone()
            },
            ChannelContract {
                message: MessageContract::new("Created", "Conflicting", "Other"),
                ..channel.clone()
            },
        ] {
            assert_eq!(
                document
                    .clone()
                    .with_channel(changed)
                    .to_json()
                    .unwrap_err()
                    .code(),
                "asyncapi_contract_invalid"
            );
        }
        for schema in ["{", "null", "[]", "7", "\"string\""] {
            let mut malformed = channel.clone();
            malformed.message.payload.json_schema = Some(schema.into());
            assert!(
                AsyncApiDocument::new(AsyncApiInfo::new("Events", "1.0.0"))
                    .with_channel(malformed)
                    .to_json()
                    .is_err()
            );
        }
        let mut valid = channel;
        valid.message.payload.json_schema = Some("false".into());
        assert!(
            AsyncApiDocument::new(AsyncApiInfo::new("Events", "1.0.0"))
                .with_channel(valid)
                .to_json()
                .is_ok()
        );
        assert!(
            AsyncApiDocument::new(AsyncApiInfo::new("", "1"))
                .to_json()
                .is_err()
        );
    }

    #[test]
    fn websocket_asyncapi_escapes_json_and_uri_fragment_references() {
        use forklaunch_core::ws::{EventContract, MessageDirection, WebSocketContract};
        let contract = WebSocketContract::new("feed/~space here").with_event(EventContract::new(
            "update/~with space",
            MessageDirection::ServerToClient,
            "Update",
        ));
        let encoded =
            generate_asyncapi_json(&AsyncApiInfo::new("Quote\"\n\t\u{0}", "1"), &contract);
        let value: Value = serde_json::from_str(&encoded).unwrap();
        let operation = &value["operations"]["update/~with space"];
        assert_eq!(
            operation["channel"]["$ref"],
            "#/channels/feed.~0space%20here"
        );
        assert_eq!(
            operation["messages"][0]["$ref"],
            "#/channels/feed.~0space%20here/messages/update~1~0with%20space"
        );
    }

    #[test]
    fn typed_websocket_documents_preserve_envelopes_payloads_and_controls() {
        use forklaunch_core::ws::{
            EventContract, MessageDirection, WebSocketContract, WebSocketEnvelope,
        };
        let contract = WebSocketContract::new("events")
            .with_event(EventContract::new(
                "submit",
                MessageDirection::ClientToServer,
                "Item",
            ))
            .with_event(EventContract::new(
                "created",
                MessageDirection::ServerToClient,
                "Item",
            ))
            .with_error_schema("Error")
            .with_close_schema("Close");
        let schemas = BTreeMap::from([
            (
                "Item".into(),
                json!({"type":"object", "required":["id"], "properties":{"id":{"type":"integer"}}}),
            ),
            ("Error".into(), json!({"type":"string"})),
            (
                "Close".into(),
                json!({"type":"object", "properties":{"reason":{"type":"string"}}}),
            ),
        ]);
        let encoded = generate_asyncapi_json_with_schemas(
            &AsyncApiInfo::new("Events", "1.0.0"),
            &contract,
            &schemas,
        )
        .unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["operations"].as_object().unwrap().len(), 4);
        assert_eq!(value["operations"]["operation1"]["action"], "receive");
        assert_eq!(value["operations"]["operation2"]["action"], "send");
        assert_eq!(value["operations"]["operation3"]["action"], "send");
        let messages = &value["channels"]["events"]["messages"];
        for event in ["submit", "created", "error", "close"] {
            assert_eq!(
                messages[event]["payload"]["properties"]["event"]["const"],
                event
            );
        }
        assert_eq!(
            messages["submit"]["payload"]["properties"]["payload"]["$ref"],
            "#/components/schemas/Item"
        );
        assert_eq!(value["components"]["schemas"]["Item"], schemas["Item"]);
        let frame = WebSocketEnvelope::new("submit", r#"{"id":42}"#);
        let frame: Value = serde_json::from_str(&frame.to_json()).unwrap();
        assert_eq!(frame["event"], "submit");
        assert_eq!(frame["payload"]["id"], 42);
        if let Ok(path) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_DOCUMENT") {
            std::fs::write(format!("{path}.typed.json"), encoded).unwrap();
        }
    }

    #[test]
    fn typed_websocket_documents_reject_missing_and_unsafe_schema_references() {
        use forklaunch_core::ws::{EventContract, MessageDirection, WebSocketContract};
        let contract = WebSocketContract::new("events").with_event(EventContract::new(
            "submit",
            MessageDirection::ClientToServer,
            "Item",
        ));
        let info = AsyncApiInfo::new("Events", "1");
        assert!(generate_asyncapi_json_with_schemas(&info, &contract, &BTreeMap::new()).is_err());
        for schema in [
            json!({"$ref":"https://example.com/schema.json"}),
            json!({"$ref":"#/components/schemas/Missing"}),
            json!([]),
            json!({"$ref":3}),
        ] {
            assert!(
                generate_asyncapi_json_with_schemas(
                    &info,
                    &contract,
                    &BTreeMap::from([("Item".into(), schema)])
                )
                .is_err()
            );
        }
        let schemas = BTreeMap::from([("Item".into(), json!({"type":"integer"}))]);
        let duplicate = contract.clone().with_event(contract.events[0].clone());
        assert!(generate_asyncapi_json_with_schemas(&info, &duplicate, &schemas).is_err());
        let reserved = WebSocketContract::new("events").with_event(EventContract::new(
            "error",
            MessageDirection::ServerToClient,
            "Item",
        ));
        assert!(generate_asyncapi_json_with_schemas(&info, &reserved, &schemas).is_err());
        let mut invalid_name = schemas.clone();
        invalid_name.insert("bad/name".into(), json!({}));
        assert!(generate_asyncapi_json_with_schemas(&info, &contract, &invalid_name).is_err());
        let recursive = BTreeMap::from([(
            "Item".into(),
            json!({"type":"object","properties":{"next":{"$ref":"#/components/schemas/Item"}}}),
        )]);
        assert!(generate_asyncapi_json_with_schemas(&info, &contract, &recursive).is_ok());
    }

    #[test]
    fn generates_asyncapi_30_from_websocket_contract() {
        use forklaunch_core::ws::{EventContract, MessageDirection, WebSocketContract};

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

        let document = generate_asyncapi_json(&AsyncApiInfo::new("Realtime", "1.0.0"), &contract);

        assert!(document.contains("\"asyncapi\":\"3.0.0\""));
        // client-to-server event is received by the server
        assert!(document.contains("\"subscribe\":{\"action\":\"receive\""));
        // server-to-client event is sent by the server
        assert!(document.contains("\"user.updated\":{\"action\":\"send\""));
        assert!(document.contains("\"#/components/messages/SubscribeRequest\""));
        assert!(document.contains("\"channels\":{\"realtime\":{\"address\":\"/realtime\""));
    }

    #[test]
    fn asyncapi_30_output_is_deterministic() {
        use forklaunch_core::ws::{EventContract, MessageDirection, WebSocketContract};

        let contract = WebSocketContract::new("chat").with_event(EventContract::new(
            "message",
            MessageDirection::ClientToServer,
            "ChatMessage",
        ));
        let info = AsyncApiInfo::new("Chat", "2.0.0");

        assert_eq!(
            generate_asyncapi_json(&info, &contract),
            generate_asyncapi_json(&info, &contract)
        );
    }
}
