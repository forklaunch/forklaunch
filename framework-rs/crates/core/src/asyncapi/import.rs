//! Explicit server-view imports for JSON and binary WebSocket applications.
// Embedded schema: @asyncapi/specs 6.11.1, schemas/3.0.0-without-$id.json,
// copied unchanged (SHA-256 abe96881dbfaad495ccbb6bd8d3fb43ca18a13b9d7564849ccb66f39d5e5b20f).
// Upstream: https://github.com/asyncapi/spec-json-schemas
// Distributed under the adjacent ASYNCAPI_SPEC_LICENSE (Apache-2.0).
// Upstream attribution is retained in ASYNCAPI_SPEC_NOTICE.
mod traits;
use super::{MessageSource, RuntimeExpression};
use std::collections::{BTreeMap, BTreeSet};

use forklaunch_common::{FrameworkError, FrameworkResult};
use forklaunch_core::ws::{SocketContract, SocketContractArtifact, SocketSchemaEntry};
use serde_json::{Map, Value, json};

struct Channel {
    name: String,
    address: String,
    messages: BTreeMap<String, Value>,
    binary: BTreeSet<String>,
    receive: BTreeSet<String>,
    send: BTreeSet<String>,
    correlations: BTreeMap<String, RuntimeExpression>,
}

/// Checked socket contracts and application-owned correlation expressions.
/// Keys are channel-message JSON Pointers after resolving channel references
/// (without the leading `#`); message references retain their channel identity.
/// These expressions do not start a request manager or introduce wire envelopes.
pub struct ImportedWebSocketAsyncApi {
    pub contracts: SocketContractArtifact,
    pub correlations: BTreeMap<String, RuntimeExpression>,
}

/// Import an `AsyncAPI` 3.0 document explicitly describing a WebSocket server.
/// Server receive/send operations become client outgoing/incoming contracts.
/// JSON payloads use explicit JSON encoding and provably disjoint message
/// schemas. Binary directions require one canonical string/binary payload with
/// application/octet-stream media. Fixed replies populate the opposite direction
/// of their declared channel, including separate WebSocket paths. Each path is
/// an independent socket; connection management and request correlation remain
/// application-owned. Missing directions are forbidden.
///
/// # Errors
/// Rejects unsupported protocols, media, upgrade parameters/authentication,
/// dynamic reply addresses, application headers, overlapping messages, invalid local
/// references, unsupported resource scopes, mixed runtime dialects and bounded
/// schema/output limits.
pub fn import_server_websocket_asyncapi(
    document: &Value,
) -> FrameworkResult<SocketContractArtifact> {
    Ok(import_server_websocket_asyncapi_with_metadata(document)?.contracts)
}

/// Import the same checked server-view profile, retaining declared correlation
/// expressions for applications that route responses themselves. Only JSON
/// payload locations are supported: raw sockets have no application headers.
/// # Errors
/// Returns socket import errors, plus malformed or unsupported correlation sources.
pub fn import_server_websocket_asyncapi_with_metadata(
    document: &Value,
) -> FrameworkResult<ImportedWebSocketAsyncApi> {
    validate_document(document)?;
    if document.get("asyncapi").and_then(Value::as_str) != Some("3.0.0") {
        return Err(invalid("WebSocket import requires AsyncAPI 3.0.0"));
    }
    let info = document
        .get("info")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("AsyncAPI info is required"))?;
    let mut artifact = SocketContractArtifact::new(text(info, "title")?, text(info, "version")?);
    let mut budget = OutputBudget(32 * 1024 * 1024);
    let mut channels = read_channels(document, &mut budget)?;
    read_operations(document, &mut channels, &mut budget)?;
    let mut correlations = BTreeMap::new();
    for mut channel in channels.into_values() {
        correlations.append(&mut channel.correlations);
        let used: BTreeSet<_> = channel.receive.union(&channel.send).cloned().collect();
        if used.len() != channel.messages.len() {
            return Err(invalid(
                "Every declared channel message must be selected by an operation",
            ));
        }
        let json_messages = channel
            .messages
            .iter()
            .filter(|(pointer, _)| !channel.binary.contains(*pointer))
            .map(|(pointer, schema)| (pointer.clone(), schema.clone()))
            .collect();
        require_disjoint_messages(&json_messages)?;
        let outgoing = direction_schema(&channel, &channel.receive)?;
        let incoming = direction_schema(&channel, &channel.send)?;
        budget.reserve(&outgoing.shape)?;
        budget.reserve(&incoming.shape)?;
        let receive_name = format!("{}_receive", channel.name);
        let send_name = format!("{}_send", channel.name);
        let socket = SocketContract::new(&channel.address)
            .with_client_message(
                &receive_name,
                outgoing
                    .with_channel(&channel.address)
                    .with_operation(&receive_name),
            )
            .with_server_message(
                &send_name,
                incoming
                    .with_channel(&channel.address)
                    .with_operation(&send_name),
            );
        artifact.sockets.insert(channel.name, socket);
    }
    Ok(ImportedWebSocketAsyncApi {
        contracts: artifact,
        correlations,
    })
}

fn validate_document(document: &Value) -> FrameworkResult<()> {
    static CODEC: std::sync::OnceLock<
        Result<forklaunch_core::ws::codec::PayloadCodec<Value>, String>,
    > = std::sync::OnceLock::new();
    let mut pending = vec![(document, 0)];
    let mut nodes = 100_000_usize;
    while let Some((value, depth)) = pending.pop() {
        if depth > 64 || nodes == 0 {
            return Err(invalid("AsyncAPI document exceeds depth/node limits"));
        }
        nodes -= 1;
        let children = match value {
            Value::Array(values) => values.len(),
            Value::Object(values) => values.len(),
            _ => 0,
        };
        if children > nodes.saturating_sub(pending.len()) {
            return Err(invalid("AsyncAPI document exceeds node limit"));
        }
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(values) => {
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    OutputBudget(32 * 1024 * 1024).reserve(document)?;
    let codec = CODEC
        .get_or_init(|| {
            let schema = serde_json::from_str(include_str!("asyncapi-3.0.0.json"))
                .map_err(|error| error.to_string())?;
            forklaunch_core::ws::codec::PayloadCodec::<Value>::json_schema(&schema)
                .and_then(|codec| codec.with_max_bytes(32 * 1024 * 1024))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|_| invalid("Embedded AsyncAPI schema could not be compiled"))?;
    codec
        .encode(document)
        .map_err(|_| invalid("Document does not conform to the AsyncAPI 3.0 schema"))?;
    Ok(())
}

fn dialect_version(uri: &str) -> Option<&'static str> {
    let uri = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))?;
    match uri.trim_end_matches('#') {
        "json-schema.org/draft-04/schema" => Some("draft-04"),
        "json-schema.org/draft-06/schema" => Some("draft-06"),
        "json-schema.org/draft-07/schema" => Some("draft-07"),
        "json-schema.org/draft/2019-09/schema" => Some("2019-09"),
        "json-schema.org/draft/2020-12/schema" => Some("2020-12"),
        _ => None,
    }
}

fn payload_dialect(schema: &Value) -> FrameworkResult<&'static str> {
    match schema.get("$schema") {
        None => Ok("draft-07"),
        Some(value) => value
            .as_str()
            .and_then(dialect_version)
            .ok_or_else(|| invalid("Unsupported imported JSON Schema dialect")),
    }
}

fn validate_payload_profile(schema: &Value) -> FrameworkResult<()> {
    let dialect = payload_dialect(schema)?;
    for location in forklaunch_core::contract::schema_locations(schema, false)? {
        let node = schema
            .pointer(&location)
            .ok_or_else(|| invalid("Imported schema location is missing"))?;
        if node.get("$schema").is_some() && payload_dialect(node)? != dialect {
            return Err(invalid(
                "Mixed dialects within a message require resource-scope resolution",
            ));
        }
    }
    forklaunch_core::ws::codec::PayloadCodec::<Value>::json_schema(schema)
        .map_err(|_| invalid("Imported message JSON Schema is invalid"))?;
    Ok(())
}

fn read_channels(
    document: &Value,
    budget: &mut OutputBudget,
) -> FrameworkResult<BTreeMap<String, Channel>> {
    let values = document
        .get("channels")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("AsyncAPI channels are required"))?;
    if values.is_empty() || values.len() > 64 {
        return Err(invalid("Expected 1 through 64 WebSocket channels"));
    }
    let mut channels = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut addresses = BTreeSet::new();
    for key in values.keys() {
        let name = module_name(key)?;
        if !names.insert(name.clone()) {
            return Err(invalid("Channel module names collide after normalization"));
        }
        let root = format!("/channels/{}", token(key));
        let (location, value) = object_at(document, &root)?;
        let address = text(value, "address")?.to_owned();
        if !address.starts_with('/')
            || address.contains(['?', '#', '{', '}'])
            || address.chars().any(char::is_control)
            || !addresses.insert(address.clone())
        {
            return Err(invalid(
                "Channel addresses must be unique absolute WebSocket paths without parameters",
            ));
        }
        reject_fields(value, &["parameters"])?;
        check_servers(document, value)?;
        check_bindings(document, value)?;
        let messages = value
            .get("messages")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("Channel messages are required"))?;
        if messages.is_empty() || messages.len() > 64 {
            return Err(invalid("Expected 1 through 64 messages per channel"));
        }
        let mut schemas = BTreeMap::new();
        let mut binary = BTreeSet::new();
        let mut correlations = BTreeMap::new();
        for message in messages.keys() {
            let pointer = format!("{location}/messages/{}", token(message));
            let (schema, correlation) = message_schema(document, &pointer, budget)?;
            if let Some(correlation) = correlation {
                // Retained location and decoded pointer each consume bounded
                // output space; decoded UTF-8 is never longer than URI bytes.
                let encoded = Value::String(correlation.location().to_owned());
                budget.reserve(&encoded)?;
                budget.reserve(&encoded)?;
                correlations.insert(pointer.clone(), correlation);
            }
            budget.reserve(&schema.shape)?;
            if schema.binary {
                binary.insert(pointer.clone());
            }
            schemas.insert(pointer, schema.shape);
        }
        channels.insert(
            root,
            Channel {
                name,
                address,
                messages: schemas,
                binary,
                receive: BTreeSet::new(),
                send: BTreeSet::new(),
                correlations,
            },
        );
    }
    Ok(channels)
}

fn read_operations(
    document: &Value,
    channels: &mut BTreeMap<String, Channel>,
    budget: &mut OutputBudget,
) -> FrameworkResult<()> {
    let operations = document
        .get("operations")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("AsyncAPI operations are required"))?;
    if operations.is_empty() || operations.len() > 1024 {
        return Err(invalid("Expected 1 through 1024 operations"));
    }
    for name in operations.keys() {
        let location = format!("/operations/{}", token(name));
        let (location, operation) = object_at(document, &location)?;
        let operation = traits::apply(document, &location, operation, false, budget)?;
        reject_fields(&operation, &["security"])?;
        check_bindings(document, &operation)?;
        let channel_pointer = reference(
            operation
                .get("channel")
                .ok_or_else(|| invalid("Operation channel reference is required"))?,
        )?;
        let channel = channels
            .get_mut(&channel_pointer)
            .ok_or_else(|| invalid("Operation channel must reference a root channel"))?;
        let receive = match text(&operation, "action")? {
            "receive" => true,
            "send" => false,
            _ => return Err(invalid("Operation action must be send or receive")),
        };
        assign_messages(channel, operation.get("messages"), receive)?;
        if let Some(reply) = operation.get("reply") {
            let reply = referenced_object(document, reply)?;
            for field in reply.keys() {
                if !field.starts_with("x-") && !matches!(field.as_str(), "channel" | "messages") {
                    return Err(invalid(
                        "Only fixed WebSocket replies without dynamic addresses are supported",
                    ));
                }
            }
            let reply_channel = reference(
                reply
                    .get("channel")
                    .ok_or_else(|| invalid("Explicit reply channel reference is required"))?,
            )?;
            let channel = channels
                .get_mut(&reply_channel)
                .ok_or_else(|| invalid("Reply channel must reference a root channel"))?;
            assign_messages(channel, reply.get("messages"), !receive)?;
        }
    }
    Ok(())
}

fn assign_messages(
    channel: &mut Channel,
    messages: Option<&Value>,
    receive: bool,
) -> FrameworkResult<()> {
    let messages = messages
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty() && messages.len() <= 64)
        .ok_or_else(|| {
            invalid("Expected 1 through 64 explicit operation/reply message references")
        })?;
    for message in messages {
        let pointer = reference(message)?;
        if !channel.messages.contains_key(&pointer) {
            return Err(invalid(
                "Operation/reply messages must reference messages in its channel",
            ));
        }
        if receive {
            channel.receive.insert(pointer);
        } else {
            channel.send.insert(pointer);
        }
    }
    Ok(())
}

fn message_schema(
    document: &Value,
    pointer: &str,
    budget: &mut OutputBudget,
) -> FrameworkResult<(SocketSchemaEntry, Option<RuntimeExpression>)> {
    let (location, message) = object_at(document, pointer)?;
    let message = traits::apply(document, &location, message, true, budget)?;
    reject_fields(&message, &["headers"])?;
    let correlation = message
        .get("correlationId")
        .map(|value| {
            let object = referenced_object(document, value)?;
            let expression = RuntimeExpression::parse(text(object, "location")?)
                .map_err(|_| invalid("Invalid message correlation runtime expression"))?;
            if expression.source() != MessageSource::Payload {
                return Err(invalid(
                    "Raw WebSocket correlations require payload locations",
                ));
            }
            Ok(expression)
        })
        .transpose()?;
    check_bindings(document, &message)?;
    let media = message
        .get("contentType")
        .or_else(|| document.get("defaultContentType"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Explicit message content type is required"))?;
    if media == "application/octet-stream" {
        if correlation.is_some() {
            return Err(invalid(
                "Binary WebSocket payload correlations are unsupported",
            ));
        }
        // Only canonical byte schemas are supported: never discard assertions
        // such as minLength or enum while converting to an unchecked byte codec.
        let (_, payload) = object_at(document, &format!("{location}/payload"))?;
        if Value::Object(payload.clone()) != SocketSchemaEntry::binary().shape {
            return Err(invalid(
                "Binary messages require the canonical string/binary payload schema",
            ));
        }
        return Ok((SocketSchemaEntry::binary(), None));
    }
    if media != "application/json"
        && !(media.starts_with("application/") && media.ends_with("+json"))
    {
        return Err(invalid(
            "Only JSON or application/octet-stream WebSocket message media are supported",
        ));
    }
    let payload = message
        .get("payload")
        .ok_or_else(|| invalid("Message payload schema is required"))?;
    let schema_location = if let Some(format) = payload.get("schemaFormat") {
        let schema = payload
            .get("schema")
            .ok_or_else(|| invalid("Multi Format schema is required"))?;
        let dialect = schema
            .get("$schema")
            .and_then(Value::as_str)
            .and_then(dialect_version)
            .ok_or_else(|| invalid("Multi Format schemas require an explicit supported $schema"))?;
        if format.as_str() != Some(format!("application/schema+json;version={dialect}").as_str()) {
            return Err(invalid(
                "Multi Format schemaFormat must match the payload dialect",
            ));
        }
        format!("{location}/payload/schema")
    } else {
        format!("{location}/payload")
    };
    let schema =
        forklaunch_core::codegen::bundle_document_schema_resources(document, &schema_location)
            .map_err(|error| invalid(&format!("Imported schema resources: {error}")))?;
    validate_payload_profile(&schema)?;
    Ok((SocketSchemaEntry::json(schema), correlation))
}

fn check_servers(document: &Value, channel: &Map<String, Value>) -> FrameworkResult<()> {
    let servers = document
        .get("servers")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("Explicit WebSocket server protocols are required"))?;
    let selected = channel.get("servers").and_then(Value::as_array);
    let pointers: Vec<_> = if let Some(selected) = selected.filter(|items| !items.is_empty()) {
        selected
            .iter()
            .map(reference)
            .collect::<FrameworkResult<_>>()?
    } else {
        servers
            .keys()
            .map(|key| format!("/servers/{}", token(key)))
            .collect()
    };
    if pointers.is_empty() {
        return Err(invalid("At least one WebSocket server is required"));
    }
    for pointer in pointers {
        if !servers
            .keys()
            .any(|key| pointer == format!("/servers/{}", token(key)))
        {
            return Err(invalid("Channel servers must reference root servers"));
        }
        let (_, server) = object_at(document, &pointer)?;
        check_bindings(document, server)?;
        if !matches!(text(server, "protocol")?, "ws" | "wss") {
            return Err(invalid("Only ws/wss server protocols are supported"));
        }
        reject_fields(server, &["security", "variables"])?;
    }
    Ok(())
}

fn check_bindings(document: &Value, value: &Map<String, Value>) -> FrameworkResult<()> {
    let Some(bindings) = value.get("bindings") else {
        return Ok(());
    };
    let bindings = referenced_object(document, bindings)?;
    for key in bindings.keys() {
        if key.starts_with("x-") {
            continue;
        }
        if key != "ws" {
            return Err(invalid("Only WebSocket protocol bindings are supported"));
        }
        let binding = referenced_object(document, &bindings[key])?;
        for (key, value) in binding {
            if key.starts_with("x-") {
                continue;
            }
            match key.as_str() {
                "method" if value == "GET" => {}
                "bindingVersion" if value == "0.1.0" => {}
                _ => {
                    return Err(invalid(
                        "Only GET WebSocket bindings without upgrade parameter schemas are supported",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn referenced_object<'a>(
    document: &'a Value,
    value: &'a Value,
) -> FrameworkResult<&'a Map<String, Value>> {
    if value.get("$ref").is_some() {
        return Ok(object_at(document, &reference(value)?)?.1);
    }
    value
        .as_object()
        .ok_or_else(|| invalid("AsyncAPI referenced values must be objects"))
}

fn direction_schema(
    channel: &Channel,
    selected: &BTreeSet<String>,
) -> FrameworkResult<SocketSchemaEntry> {
    if selected.is_empty() {
        return Ok(SocketSchemaEntry::json(json!(false)));
    }
    if !selected.is_disjoint(&channel.binary) {
        if selected.len() != 1 {
            return Err(invalid(
                "Binary directions require exactly one message; mixed wire types and ambiguous byte messages are unsupported",
            ));
        }
        return Ok(SocketSchemaEntry::binary());
    }
    if selected.len() == 1 {
        return Ok(SocketSchemaEntry::json(
            channel.messages[selected.first().unwrap()].clone(),
        ));
    }
    let first = &channel.messages[selected.first().unwrap()];
    let dialect = payload_dialect(first)?;
    let mut variants = Vec::new();
    let mut budget = OutputBudget(32 * 1024 * 1024);
    for pointer in selected {
        let schema = &channel.messages[pointer];
        if payload_dialect(schema)? != dialect {
            return Err(invalid(
                "Message alternatives in one direction must use the same dialect",
            ));
        }
        budget.reserve(schema)?;
        let index = variants.len().to_string();
        let relocated = forklaunch_core::contract::relocate_schema_json(
            &schema.to_string(),
            &["oneOf", &index],
        );
        variants.push(
            serde_json::from_str::<Value>(&relocated)
                .map_err(|_| invalid("Imported schema relocation failed"))?,
        );
    }
    let mut schema = json!({"oneOf":variants});
    if let Some(declaration) = first.get("$schema") {
        schema["$schema"] = declaration.clone();
    }
    Ok(SocketSchemaEntry::json(schema))
}

fn require_disjoint_messages(messages: &BTreeMap<String, Value>) -> FrameworkResult<()> {
    let schemas: Vec<_> = messages.values().collect();
    for (index, left) in schemas.iter().enumerate() {
        for right in &schemas[index + 1..] {
            let left_types = allowed_types(left, left)?;
            let right_types = allowed_types(right, right)?;
            if left_types.is_disjoint(&right_types)
                || (left_types == BTreeSet::from(["object"])
                    && right_types == BTreeSet::from(["object"])
                    && disjoint_tags(left, right)?)
            {
                continue;
            }
            return Err(invalid(
                "Channel messages require provably disjoint types or required string discriminators",
            ));
        }
    }
    Ok(())
}

fn allowed_types<'a>(root: &'a Value, schema: &'a Value) -> FrameworkResult<BTreeSet<&'a str>> {
    let schema = dereference_schema(root, schema)?;
    if schema == &json!(false) {
        return Ok(BTreeSet::new());
    }
    let types: Vec<_> = match schema.get("type") {
        Some(Value::String(value)) => vec![value.as_str()],
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| invalid("Schema types must be strings"))
            })
            .collect::<FrameworkResult<_>>()?,
        _ => {
            return Err(invalid(
                "Imported message schemas require explicit root types",
            ));
        }
    };
    types
        .into_iter()
        .map(|value| match value {
            "integer" => Ok("number"),
            "object" | "array" | "number" | "boolean" | "null" | "string" => Ok(value),
            _ => Err(invalid("Unsupported message schema type")),
        })
        .collect()
}

fn disjoint_tags(left: &Value, right: &Value) -> FrameworkResult<bool> {
    let left_root = left;
    let right_root = right;
    let left = dereference_schema(left_root, left)?;
    let right = dereference_schema(right_root, right)?;
    let Some(left_required) = left.get("required").and_then(Value::as_array) else {
        return Ok(false);
    };
    let Some(right_required) = right.get("required").and_then(Value::as_array) else {
        return Ok(false);
    };
    for name in left_required.iter().filter_map(Value::as_str) {
        if !right_required
            .iter()
            .any(|value| value.as_str() == Some(name))
        {
            continue;
        }
        let left = string_tags(left_root, &left["properties"][name])?;
        let right = string_tags(right_root, &right["properties"][name])?;
        if let (Some(left), Some(right)) = (left, right)
            && left.is_disjoint(&right)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn string_tags<'a>(
    root: &'a Value,
    schema: &'a Value,
) -> FrameworkResult<Option<BTreeSet<&'a str>>> {
    let schema = dereference_schema(root, schema)?;
    if payload_dialect(root)? != "draft-04"
        && let Some(value) = schema.get("const").and_then(Value::as_str)
    {
        return Ok(Some(BTreeSet::from([value])));
    }
    Ok(schema
        .get("enum")
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .and_then(|values| {
            values
                .iter()
                .map(Value::as_str)
                .collect::<Option<BTreeSet<_>>>()
        }))
}

fn dereference_schema<'a>(root: &'a Value, mut schema: &'a Value) -> FrameworkResult<&'a Value> {
    for _ in 0..64 {
        let Some(reference_value) = schema.get("$ref") else {
            return Ok(schema);
        };
        let pointer = pointer(
            reference_value
                .as_str()
                .ok_or_else(|| invalid("Schema reference must be a string"))?,
        )?;
        schema = root
            .pointer(&pointer)
            .ok_or_else(|| invalid("Schema reference is missing"))?;
    }
    Err(invalid("Schema reference chain exceeds 64 steps"))
}

fn object_at<'a>(
    document: &'a Value,
    location: &str,
) -> FrameworkResult<(String, &'a Map<String, Value>)> {
    let mut location = location.to_owned();
    let mut seen = BTreeSet::new();
    for _ in 0..64 {
        if !seen.insert(location.clone()) {
            return Err(invalid("AsyncAPI reference cycle"));
        }
        let value = document
            .pointer(&location)
            .ok_or_else(|| invalid("AsyncAPI reference target is missing"))?;
        let object = value
            .as_object()
            .ok_or_else(|| invalid("AsyncAPI objects must be maps"))?;
        if object.contains_key("$ref") {
            location = reference(value)?;
        } else {
            return Ok((location, object));
        }
    }
    Err(invalid("AsyncAPI reference chain exceeds 64 steps"))
}

fn reference(value: &Value) -> FrameworkResult<String> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("Expected an AsyncAPI Reference Object"))?;
    if object.len() != 1 {
        return Err(invalid("AsyncAPI reference siblings are unsupported"));
    }
    pointer(
        object
            .get("$ref")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("AsyncAPI reference must be a string"))?,
    )
}
fn pointer(reference: &str) -> FrameworkResult<String> {
    let fragment = reference
        .strip_prefix('#')
        .ok_or_else(|| invalid("Only document-local AsyncAPI references are supported"))?;
    if !fragment.starts_with('/') || fragment.to_ascii_lowercase().contains("%2f") {
        return Err(invalid(
            "AsyncAPI references require literal JSON pointer separators",
        ));
    }
    let pointer = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .map_err(|_| invalid("AsyncAPI reference is not UTF-8"))?;
    if !pointer.starts_with('/') {
        return Err(invalid("AsyncAPI references require JSON pointers"));
    }
    Ok(pointer.into_owned())
}
fn token(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn text<'a>(object: &'a Map<String, Value>, field: &str) -> FrameworkResult<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 1024)
        .ok_or_else(|| invalid("Required AsyncAPI string is missing or exceeds 1024 bytes"))
}
fn reject_fields(object: &Map<String, Value>, fields: &[&str]) -> FrameworkResult<()> {
    for field in fields {
        if let Some(value) = object.get(*field) {
            let empty = value.as_array().is_some_and(Vec::is_empty)
                || value.as_object().is_some_and(Map::is_empty);
            if !empty {
                return Err(invalid(
                    "AsyncAPI security, traits, replies, headers or parameters require additional wire adapters",
                ));
            }
        }
    }
    Ok(())
}
fn module_name(value: &str) -> FrameworkResult<String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid(
            "Channel keys must be ASCII module names of 1 through 128 bytes",
        ));
    }
    Ok(value.replace('-', "_").to_ascii_lowercase())
}
struct OutputBudget(usize);
impl OutputBudget {
    fn reserve(&mut self, value: &Value) -> FrameworkResult<()> {
        serde_json::to_writer(self, value)
            .map_err(|_| invalid("Imported socket schemas exceed 32 MiB"))
    }
}
impl std::io::Write for OutputBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("AsyncAPI schema byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn invalid(message: &str) -> FrameworkError {
    FrameworkError::new("asyncapi_socket_import_invalid", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::ws::codec::PayloadCodec;

    #[test]
    fn correlation_metadata_resolves_references_and_trait_precedence() {
        let mut doc = document();
        doc["components"]["correlationIds"] =
            json!({"id":{"location":"$message.payload#/message"}});
        doc["components"]["messageTraits"] =
            json!({"trace":{"correlationId":{"location":"$message.payload#/kind"}}});
        doc["components"]["messages"]["Request"]["traits"] =
            json!([{"$ref":"#/components/messageTraits/trace"}]);
        doc["components"]["messages"]["Request"]["correlationId"] =
            json!({"$ref":"#/components/correlationIds/id"});
        doc["components"]["messages"]["Reply"]["traits"] =
            json!([{"$ref":"#/components/messageTraits/trace"}]);
        let imported = import_server_websocket_asyncapi_with_metadata(&doc).unwrap();
        assert_eq!(imported.correlations.len(), 2);
        let request = &imported.correlations["/channels/chat/messages/request"];
        assert_eq!(request.location(), "$message.payload#/message");
        assert_eq!(
            request.resolve(None, Some(&json!({"kind":"request","message":17}))),
            Some(&json!(17))
        );
        assert_eq!(
            imported.correlations["/channels/chat/messages/reply"].location(),
            "$message.payload#/kind"
        );
        let ordinary = import_server_websocket_asyncapi(&doc).unwrap();
        assert_eq!(ordinary, imported.contracts);
        if let Ok(path) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_CORRELATION_DOCUMENT") {
            std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
        }
    }

    #[test]
    fn correlation_metadata_rejects_unrepresentable_or_malformed_sources() {
        for location in [
            "$message.header#/id",
            "$message.payload#/bad~2",
            "$message.payload#/%FF",
            "$message.payload.extra",
        ] {
            let mut doc = document();
            doc["components"]["messages"]["Request"]["correlationId"] =
                json!({"location":location});
            assert!(
                import_server_websocket_asyncapi_with_metadata(&doc).is_err(),
                "{location}"
            );
        }
        let mut doc = document();
        doc["components"]["messages"]["Request"]["correlationId"] =
            json!({"$ref":"#/components/correlationIds/missing"});
        assert!(import_server_websocket_asyncapi_with_metadata(&doc).is_err());
        let mut doc = document();
        doc["components"]["messages"]["Request"]["contentType"] = json!("application/octet-stream");
        doc["components"]["messages"]["Request"]["payload"] =
            json!({"type":"string","format":"binary"});
        doc["components"]["messages"]["Request"]["correlationId"] =
            json!({"location":"$message.payload"});
        assert!(import_server_websocket_asyncapi_with_metadata(&doc).is_err());
    }

    fn document() -> Value {
        json!({
            "asyncapi":"3.0.0", "info":{"title":"Chat", "version":"1.0.0"},
            "defaultContentType":"application/json",
            "servers":{"local":{"host":"127.0.0.1:8080","protocol":"ws"}},
            "channels":{"chat":{"address":"/chat", "bindings":{"ws":{"method":"GET","bindingVersion":"0.1.0"}}, "messages":{
                "request":{"$ref":"#/components/messages/Request"},
                "reply":{"$ref":"#/components/messages/Reply"}
            }}},
            "operations":{
                "accept":{"action":"receive","channel":{"$ref":"#/channels/chat"},"messages":[{"$ref":"#/channels/chat/messages/request"}]},
                "reply":{"action":"send","channel":{"$ref":"#/channels/chat"},"messages":[{"$ref":"#/channels/chat/messages/reply"}]}
            },
            "components":{"messages":{
                "Request":{"payload":{"$ref":"#/components/schemas/Request"}},
                "Reply":{"payload":{"$ref":"#/components/schemas/Reply"}}
            }, "schemas":{
                "Request":{"type":"object","required":["kind","message"],"properties":{"kind":{"const":"request"},"message":{"type":"string","minLength":1}},"additionalProperties":false},
                "Reply":{"type":"object","required":["kind","accepted"],"properties":{"kind":{"const":"reply"},"accepted":{"type":"boolean"}},"additionalProperties":false}
            }}
        })
    }

    fn reply_document(receive: bool, referenced: bool) -> Value {
        let mut input = document();
        let (operation, removed, message) = if receive {
            ("accept", "reply", "reply")
        } else {
            ("reply", "accept", "request")
        };
        input["operations"].as_object_mut().unwrap().remove(removed);
        let reply = json!({
            "channel":{"$ref":"#/channels/chat"},
            "messages":[{"$ref":format!("#/channels/chat/messages/{message}")}]
        });
        if referenced {
            input["components"]["replies"] = json!({"response":reply});
            input["operations"][operation]["reply"] =
                json!({"$ref":"#/components/replies/response"});
        } else {
            input["operations"][operation]["reply"] = reply;
        }
        input
    }

    #[test]
    fn fixed_same_channel_replies_preserve_both_message_directions() {
        for binary in [false, true] {
            let with_media = |mut input: Value| {
                if binary {
                    input["defaultContentType"] = json!("application/octet-stream");
                    for name in ["Request", "Reply"] {
                        input["components"]["schemas"][name] =
                            json!({"type":"string","format":"binary"});
                    }
                }
                input
            };
            let expected = serde_json::to_value(
                import_server_websocket_asyncapi(&with_media(document())).unwrap(),
            )
            .unwrap();
            for receive in [false, true] {
                for referenced in [false, true] {
                    let input = with_media(reply_document(receive, referenced));
                    let original = input.clone();
                    let artifact = import_server_websocket_asyncapi(&input).unwrap();
                    assert_eq!(serde_json::to_value(artifact).unwrap(), expected);
                    assert_eq!(input, original);
                }
            }
        }
    }

    #[test]
    fn fixed_cross_channel_replies_keep_independent_paths_and_directions() {
        for receive in [false, true] {
            for referenced in [false, true] {
                for binary in [false, true] {
                    let mut input = reply_document(receive, referenced);
                    let (operation, reply_message, request_message) = if receive {
                        ("accept", "reply", "request")
                    } else {
                        ("reply", "request", "reply")
                    };
                    let reply = input["channels"]["chat"]["messages"][reply_message].take();
                    input["channels"]["chat"]["messages"]
                        .as_object_mut()
                        .unwrap()
                        .remove(reply_message);
                    input["channels"]["responses"] = json!({
                        "address":"/responses",
                        "messages":{reply_message:reply}
                    });
                    let reply = json!({
                        "channel":{"$ref":"#/channels/responses"},
                        "messages":[{"$ref":format!("#/channels/responses/messages/{reply_message}")}]
                    });
                    if referenced {
                        input["components"]["replies"]["response"] = reply;
                    } else {
                        input["operations"][operation]["reply"] = reply;
                    }
                    if binary {
                        input["defaultContentType"] = json!("application/octet-stream");
                        for name in ["Request", "Reply"] {
                            input["components"]["schemas"][name] =
                                json!({"type":"string","format":"binary"});
                        }
                    }
                    let original = input.clone();
                    let artifact = import_server_websocket_asyncapi(&input).unwrap();
                    assert_eq!(input, original);
                    assert_eq!(artifact.sockets.len(), 2);
                    for (name, allowed_outgoing, message) in [
                        ("chat", receive, request_message),
                        ("responses", !receive, reply_message),
                    ] {
                        let socket = &artifact.sockets[name];
                        let path = if name == "chat" {
                            "/chat"
                        } else {
                            "/responses"
                        };
                        assert_eq!(socket.path, path);
                        let outgoing = &socket.client_messages[&format!("{name}_receive")];
                        let incoming = &socket.server_messages[&format!("{name}_send")];
                        let (allowed, forbidden) = if allowed_outgoing {
                            (outgoing, incoming)
                        } else {
                            (incoming, outgoing)
                        };
                        assert_eq!(forbidden.shape, json!(false));
                        if binary {
                            assert_eq!(allowed.shape, SocketSchemaEntry::binary().shape);
                        } else {
                            let body = if message == "request" {
                                json!({"kind":"request","message":"hello"})
                            } else {
                                json!({"kind":"reply","accepted":true})
                            };
                            let codec = PayloadCodec::<Value>::json_schema(&allowed.shape).unwrap();
                            assert!(codec.encode(&body).is_ok());
                            assert!(codec.encode(&json!({"kind":"wrong"})).is_err());
                        }
                        assert!(
                            PayloadCodec::<Value>::json_schema(&forbidden.shape)
                                .unwrap()
                                .encode(&json!(null))
                                .is_err()
                        );
                    }
                    input["operations"][operation]["reply"] = json!({
                        "channel":{"$ref":"#/channels/responses"},
                        "messages":[{"$ref":format!("#/channels/chat/messages/{request_message}")}]
                    });
                    assert!(import_server_websocket_asyncapi(&input).is_err());
                }
            }
        }
    }

    #[test]
    fn replies_reject_dynamic_foreign_ambiguous_and_unbounded_definitions() {
        for mutation in [
            "dynamic",
            "other-channel",
            "missing-channel",
            "missing-messages",
            "empty",
            "too-many",
            "foreign-message",
            "external",
            "cycle",
            "sibling",
            "overlap",
            "wrong-target",
        ] {
            let mut input = reply_document(true, false);
            match mutation {
                "dynamic" => {
                    input["operations"]["accept"]["reply"]["address"] =
                        json!({"location":"$message.header#/replyTo"});
                }
                "other-channel" => {
                    input["channels"]["other"] = input["channels"]["chat"].clone();
                    input["channels"]["other"]["address"] = json!("/other");
                    input["operations"]["accept"]["reply"]["channel"] =
                        json!({"$ref":"#/channels/other"});
                }
                "missing-channel" => {
                    input["operations"]["accept"]["reply"]
                        .as_object_mut()
                        .unwrap()
                        .remove("channel");
                }
                "missing-messages" => {
                    input["operations"]["accept"]["reply"]
                        .as_object_mut()
                        .unwrap()
                        .remove("messages");
                }
                "empty" => input["operations"]["accept"]["reply"]["messages"] = json!([]),
                "too-many" => {
                    input["operations"]["accept"]["reply"]["messages"] =
                        json!(vec![json!({"$ref":"#/channels/chat/messages/reply"}); 65]);
                }
                "foreign-message" => {
                    input["operations"]["accept"]["reply"]["messages"] =
                        json!([{"$ref":"#/components/messages/Reply"}]);
                }
                "external" => {
                    input["operations"]["accept"]["reply"] =
                        json!({"$ref":"https://example.invalid/reply"});
                }
                "cycle" => {
                    input["operations"]["accept"]["reply"] =
                        json!({"$ref":"#/operations/accept/reply"});
                }
                "sibling" => {
                    input["operations"]["accept"]["reply"]["$ref"] =
                        json!("#/components/replies/response");
                }
                "overlap" => {
                    input["components"]["schemas"]["Reply"] =
                        input["components"]["schemas"]["Request"].clone();
                }
                "wrong-target" => {
                    input["operations"]["accept"]["reply"] =
                        json!({"$ref":"#/components/messages/Reply"});
                }
                _ => unreachable!(),
            }
            assert!(
                import_server_websocket_asyncapi(&input).is_err(),
                "{mutation}"
            );
        }
    }

    #[test]
    fn server_operations_import_client_directions_and_original_constraints() {
        let input = document();
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let socket = &artifact.sockets["chat"];
        assert_eq!(socket.path, "/chat");
        let outgoing =
            PayloadCodec::<Value>::json_schema(&socket.client_messages["chat_receive"].shape)
                .unwrap();
        let incoming =
            PayloadCodec::<Value>::json_schema(&socket.server_messages["chat_send"].shape).unwrap();
        assert!(
            outgoing
                .encode(&json!({"kind":"request","message":"hello"}))
                .is_ok()
        );
        assert!(
            outgoing
                .encode(&json!({"kind":"request","message":""}))
                .is_err()
        );
        assert!(
            outgoing
                .encode(&json!({"kind":"request","message":"hello","extra":true}))
                .is_err()
        );
        assert!(
            outgoing
                .encode(&json!({"kind":"reply","accepted":true}))
                .is_err()
        );
        assert!(
            incoming
                .encode(&json!({"kind":"reply","accepted":true}))
                .is_ok()
        );
        assert_eq!(input, document());
    }

    #[test]
    fn traits_merge_in_order_with_target_precedence_and_preserve_payload_scope() {
        let mut input = document();
        input.as_object_mut().unwrap().remove("defaultContentType");
        input["components"]["messageTraits"] = json!({
            "first":{"contentType":"text/plain", "description":"first", "tags":[{"name":"first"}],
                "x-settings":{"first":true,"removed":true}},
            "second":{"contentType":"application/json", "description":"second", "tags":[{"name":"second"}],
                "x-settings":{"second":true,"removed":null}}
        });
        input["components"]["operationTraits"] = json!({
            "get":{"bindings":{"ws":{"method":"GET","bindingVersion":"0.1.0"}}, "summary":"inherited"}
        });
        for name in ["Request", "Reply"] {
            input["components"]["messages"][name]["traits"] = json!([
                {"$ref":"#/components/messageTraits/first"},
                {"$ref":"#/components/messageTraits/second"}
            ]);
            input["components"]["messages"][name]["description"] = json!("target");
            input["components"]["messages"][name]["x-settings"] = json!({"target":true});
        }
        input["operations"]["accept"]["traits"] =
            json!([{"$ref":"#/components/operationTraits/get"}]);
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let baseline = import_server_websocket_asyncapi(&document()).unwrap();
        assert_eq!(
            serde_json::to_value(artifact).unwrap(),
            serde_json::to_value(baseline).unwrap()
        );
        let mut budget = OutputBudget(32 * 1024 * 1024);
        let location = "/components/messages/Request";
        let merged = traits::apply(
            &input,
            location,
            input.pointer(location).unwrap().as_object().unwrap(),
            true,
            &mut budget,
        )
        .unwrap();
        assert_eq!(merged["contentType"], "application/json");
        assert_eq!(merged["description"], "target");
        assert_eq!(merged["tags"], json!([{"name":"second"}]));
        assert_eq!(
            merged["x-settings"],
            json!({"first":true,"second":true,"target":true})
        );
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_TRAITS") {
            std::fs::write(
                output,
                serde_json::to_vec(&json!({"document":input,"message":merged.into_owned()}))
                    .unwrap(),
            )
            .unwrap();
        }
        // The target's content type wins over the traits as well.
        input["components"]["messages"]["Request"]["contentType"] = json!("text/plain");
        assert!(import_server_websocket_asyncapi(&input).is_err());
    }

    #[test]
    fn inherited_wire_requirements_and_bad_trait_references_are_not_silently_dropped() {
        for mutation in [
            "headers", "security", "binding", "cycle", "external", "payload", "too-many",
        ] {
            let mut input = document();
            match mutation {
                "headers" => {
                    input["components"]["messages"]["Request"]["traits"] =
                        json!([{"headers":{"type":"object"}}]);
                }
                "security" => {
                    input["operations"]["accept"]["traits"] =
                        json!([{"security":[{"type":"http","scheme":"bearer"}]}]);
                }
                "binding" => {
                    input["operations"]["accept"]["traits"] =
                        json!([{"bindings":{"ws":{"method":"POST"}}}]);
                }
                "cycle" | "external" | "payload" => {
                    input["components"]["messages"]["Request"]["traits"] =
                        json!([{"$ref":"#/components/messageTraits/bad"}]);
                    input["components"]["messageTraits"]["bad"] = match mutation {
                        "cycle" => json!({"$ref":"#/components/messageTraits/bad"}),
                        "external" => json!({"$ref":"https://example.invalid/trait"}),
                        _ => json!({"$ref":"#/components/messages/Reply"}),
                    };
                }
                "too-many" => {
                    input["components"]["messages"]["Request"]["traits"] =
                        json!(vec![json!({"contentType":"application/json"}); 65]);
                }
                _ => unreachable!(),
            }
            assert!(
                import_server_websocket_asyncapi(&input).is_err(),
                "{mutation}"
            );
        }
    }

    #[test]
    fn absent_directions_are_forbidden_and_special_message_pointers_resolve() {
        let mut input = document();
        input["channels"]["chat"]["messages"]
            .as_object_mut()
            .unwrap()
            .remove("reply");
        input["operations"].as_object_mut().unwrap().remove("reply");
        let value = input["channels"]["chat"]["messages"]
            .as_object_mut()
            .unwrap()
            .remove("request")
            .unwrap();
        input["channels"]["chat"]["messages"]["request/~ 水%"] = value;
        input["operations"]["accept"]["messages"][0]["$ref"] =
            json!("#/channels/chat/messages/request~1~0%20%E6%B0%B4%25");
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        assert_eq!(
            artifact.sockets["chat"].server_messages["chat_send"].shape,
            json!(false)
        );
    }

    #[test]
    fn multiple_selected_messages_preserve_oneof_and_local_reference_bases() {
        let mut input = document();
        input["channels"]["chat"]["messages"]["other"] =
            json!({"$ref":"#/components/messages/Other"});
        input["components"]["messages"]["Other"] =
            json!({"payload":{"$ref":"#/components/schemas/Other"}});
        input["components"]["schemas"]["Other"] = json!({"type":"object","required":["kind","count"],"properties":{"kind":{"const":"other"},"count":{"type":"integer","minimum":1}},"additionalProperties":false});
        input["operations"]["accept"]["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"$ref":"#/channels/chat/messages/other"}));
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let schema = &artifact.sockets["chat"].client_messages["chat_receive"].shape;
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 2);
        let codec = PayloadCodec::<Value>::json_schema(schema).unwrap();
        assert!(
            codec
                .encode(&json!({"kind":"request","message":"hello"}))
                .is_ok()
        );
        assert!(codec.encode(&json!({"kind":"other","count":2})).is_ok());
        assert!(codec.encode(&json!({"kind":"other","count":0})).is_err());
        assert!(
            codec
                .encode(&json!({"kind":"reply","accepted":true}))
                .is_err()
        );
    }

    #[test]
    fn recursive_document_payloads_retain_constraints() {
        let mut input = document();
        input["components"]["schemas"]["Request"]["properties"]["next"] =
            json!({"$ref":"#/components/schemas/Request"});
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let schema = &artifact.sockets["chat"].client_messages["chat_receive"].shape;
        let codec = PayloadCodec::<Value>::json_schema(schema).unwrap();
        assert!(codec.encode(&json!({"kind":"request","message":"root","next":{"kind":"request","message":"child"}})).is_ok());
        assert!(codec.encode(&json!({"kind":"request","message":"root","next":{"kind":"request","message":""}})).is_err());
    }

    #[test]
    fn unsupported_wire_rules_overlap_and_bad_references_fail_checked_import() {
        for mutation in [
            "overlap",
            "array-tags",
            "external",
            "encoded-leading-separator",
            "cycle",
            "subset",
            "protocol",
            "server-binding",
            "headers",
            "address",
            "invalid-spec",
        ] {
            let mut input = document();
            match mutation {
                "overlap" => {
                    input["components"]["schemas"]["Reply"] =
                        input["components"]["schemas"]["Request"].clone();
                }
                "array-tags" => {
                    input["components"]["schemas"]["Request"]["type"] = json!("array");
                    input["components"]["schemas"]["Reply"]["type"] = json!("array");
                }
                "external" => {
                    input["components"]["messages"]["Request"]["payload"]["$ref"] =
                        json!("https://example.invalid/schema");
                }
                "encoded-leading-separator" => {
                    input["components"]["messages"]["Request"]["payload"]["$ref"] =
                        json!("#%2Fcomponents/schemas/Request");
                }
                "cycle" => {
                    input["channels"]["chat"]["messages"]["request"] =
                        json!({"$ref":"#/channels/chat/messages/request"});
                }
                "subset" => {
                    input["operations"]["accept"]["messages"][0]["$ref"] =
                        json!("#/components/messages/Request");
                }
                "protocol" => input["servers"]["local"]["protocol"] = json!("kafka"),
                "server-binding" => input["servers"]["local"]["bindings"] = json!({"kafka":{}}),
                "headers" => {
                    input["components"]["messages"]["Request"]["headers"] =
                        json!({"type":"object"});
                }
                "address" => input["channels"]["chat"]["address"] = json!("/chat/{id}"),
                "invalid-spec" => input["operations"]["accept"]["action"] = json!("invalid"),
                _ => unreachable!(),
            }
            assert!(
                import_server_websocket_asyncapi(&input).is_err(),
                "{mutation}"
            );
        }
    }

    #[test]
    fn binary_import_preserves_wire_metadata_and_rejects_ambiguous_constraints() {
        let mut input = document();
        input["defaultContentType"] = json!("application/octet-stream");
        for name in ["Request", "Reply"] {
            input["components"]["schemas"][name] = json!({"type":"string","format":"binary"});
        }
        let original = input.clone();
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let socket = &artifact.sockets["chat"];
        for entry in [
            &socket.client_messages["chat_receive"],
            &socket.server_messages["chat_send"],
        ] {
            assert!(entry.binary && entry.valid_binary_shape());
        }
        assert_eq!(input, original);
        for mutation in [
            "length",
            "enum",
            "ref-sibling",
            "encoded-separator",
            "cycle",
            "external",
            "multiple",
            "mixed",
        ] {
            let mut bad = input.clone();
            match mutation {
                "length" => bad["components"]["schemas"]["Request"]["minLength"] = json!(2),
                "enum" => bad["components"]["schemas"]["Request"]["enum"] = json!(["text"]),
                "encoded-separator" => {
                    bad["components"]["messages"]["Request"]["payload"] =
                        json!({"$ref":"#/components%2Fschemas/Request"});
                }
                "ref-sibling" => {
                    bad["components"]["messages"]["Request"]["payload"]["maxLength"] = json!(4);
                }
                "cycle" => {
                    bad["components"]["schemas"]["Request"] =
                        json!({"$ref":"#/components/schemas/Request"});
                }
                "external" => {
                    bad["components"]["messages"]["Request"]["payload"] =
                        json!({"$ref":"https://example.invalid/bytes"});
                }
                "multiple" | "mixed" => {
                    bad["operations"]["accept"]["messages"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"$ref":"#/channels/chat/messages/reply"}));
                    if mutation == "mixed" {
                        bad["components"]["messages"]["Reply"]["contentType"] =
                            json!("application/json");
                        bad["components"]["schemas"]["Reply"] =
                            document()["components"]["schemas"]["Reply"].clone();
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                import_server_websocket_asyncapi(&bad).is_err(),
                "{mutation}"
            );
        }
        input["components"]["messages"]["Reply"]["contentType"] = json!("application/json");
        input["components"]["schemas"]["Reply"] =
            document()["components"]["schemas"]["Reply"].clone();
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        assert!(artifact.sockets["chat"].client_messages["chat_receive"].binary);
        assert!(!artifact.sockets["chat"].server_messages["chat_send"].binary);
        input["operations"].as_object_mut().unwrap().remove("reply");
        input["channels"]["chat"]["messages"]
            .as_object_mut()
            .unwrap()
            .remove("reply");
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        assert_eq!(
            artifact.sockets["chat"].server_messages["chat_send"].shape,
            json!(false)
        );
    }

    #[test]
    fn multi_format_dialects_preserve_numeric_bounds_and_reject_mismatches() {
        for (version, uri) in [
            ("draft-04", "http://json-schema.org/draft-04/schema#"),
            ("draft-06", "http://json-schema.org/draft-06/schema#"),
            ("draft-07", "http://json-schema.org/draft-07/schema#"),
            ("2019-09", "https://json-schema.org/draft/2019-09/schema"),
            ("2020-12", "https://json-schema.org/draft/2020-12/schema"),
        ] {
            let mut input = document();
            input["operations"].as_object_mut().unwrap().remove("reply");
            input["channels"]["chat"]["messages"]
                .as_object_mut()
                .unwrap()
                .remove("reply");
            let bound = if version == "draft-04" {
                json!({"type":"number","minimum":0,"exclusiveMinimum":true})
            } else {
                json!({"type":"number","exclusiveMinimum":0})
            };
            input["components"]["messages"]["Request"]["payload"] = json!({
                "schemaFormat":format!("application/schema+json;version={version}"),
                "schema":{"$schema":uri,"type":"object","required":["value"],"definitions":{"Bound":bound},"properties":{"value":{"$ref":"#/components/messages/Request/payload/schema/definitions/Bound"}}}
            });
            let artifact = import_server_websocket_asyncapi(&input).unwrap();
            let schema = &artifact.sockets["chat"].client_messages["chat_receive"].shape;
            let codec = PayloadCodec::<Value>::json_schema(schema).unwrap();
            assert!(codec.encode(&json!({"value":1})).is_ok(), "{version}");
            assert!(codec.encode(&json!({"value":0})).is_err(), "{version}");
            assert!(codec.encode(&json!({"value":-1})).is_err(), "{version}");
            for bad in [
                json!("application/avro"),
                json!("application/schema+json;version=unknown"),
                json!(format!(
                    "application/schema+json;version={}",
                    if version == "draft-04" {
                        "draft-07"
                    } else {
                        "draft-04"
                    }
                )),
            ] {
                input["components"]["messages"]["Request"]["payload"]["schemaFormat"] = bad;
                assert!(import_server_websocket_asyncapi(&input).is_err());
            }
        }
    }

    #[test]
    fn draft4_message_unions_preserve_dialect_and_do_not_use_ignored_const_tags() {
        let mut input = document();
        for name in ["Request", "Reply"] {
            let mut schema = input["components"]["schemas"][name].clone();
            schema["$schema"] = json!("http://json-schema.org/draft-04/schema#");
            input["components"]["messages"][name]["payload"] = json!({
                "schemaFormat":"application/schema+json;version=draft-04", "schema":schema
            });
        }
        assert!(import_server_websocket_asyncapi(&input).is_err());
        for (name, tag) in [("Request", "request"), ("Reply", "reply")] {
            input["components"]["messages"][name]["payload"]["schema"]["properties"]["kind"] =
                json!({"enum":[tag]});
        }
        input["operations"]["accept"]["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"$ref":"#/channels/chat/messages/reply"}));
        let artifact = import_server_websocket_asyncapi(&input).unwrap();
        let schema = &artifact.sockets["chat"].client_messages["chat_receive"].shape;
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-04/schema#");
        let codec = PayloadCodec::<Value>::json_schema(schema).unwrap();
        assert!(
            codec
                .encode(&json!({"kind":"request","message":"hello"}))
                .is_ok()
        );
        assert!(
            codec
                .encode(&json!({"kind":"reply","accepted":true}))
                .is_ok()
        );
        assert!(
            codec
                .encode(&json!({"kind":"other","message":"hello"}))
                .is_err()
        );
        input["components"]["messages"]["Reply"]["payload"]["schemaFormat"] =
            json!("application/schema+json;version=draft-07");
        input["components"]["messages"]["Reply"]["payload"]["schema"]["$schema"] =
            json!("http://json-schema.org/draft-07/schema#");
        assert!(import_server_websocket_asyncapi(&input).is_err());
    }

    #[test]
    fn asyncapi_import_conformance() {
        let Ok(input) = std::env::var("FORKLAUNCH_TEST_ASYNCAPI_IMPORT_DOCUMENT") else {
            return;
        };
        let document = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let artifact = import_server_websocket_asyncapi(&document).unwrap();
        std::fs::write(
            std::env::var("FORKLAUNCH_TEST_ASYNCAPI_IMPORT_ARTIFACT").unwrap(),
            serde_json::to_vec(&artifact).unwrap(),
        )
        .unwrap();
    }
}
