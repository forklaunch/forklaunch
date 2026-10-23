//! Native clients for checked raw socket contract plans.
use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::{Result, bail};
use forklaunch_core::codegen::{
    render_rust_models, response::validate_identifier, websocket::SocketSdkPlan,
};

use crate::rust_sdk::RustSdkOptions;

/// Generate a typed client sharing the native transport's control and cancellation behavior.
///
/// Consumers need `forklaunch-ws` (`transport`, `json-schema`), serde (`derive`),
/// `serde_json`, tokio (`net`) and tokio-tungstenite 0.30. Date-bearing contracts
/// also use forklaunch-universal-sdk for the shared checked chrono codec. Caller requests retain
/// authorization headers and subprotocols; connect returns the upgrade response.
///
/// # Errors
/// Rejects invalid client identifiers, conflicting type names and model rendering failures.
#[allow(clippy::similar_names)]
pub fn generate_rust_websocket_sdk_source(
    plan: &SocketSdkPlan,
    options: &RustSdkOptions,
) -> Result<String> {
    validate_identifier(&options.client_struct)?;
    let payloads = [
        &plan.incoming,
        &plan.outgoing,
        &plan.ping,
        &plan.pong,
        &plan.close,
    ];
    let mut source = String::from(
        "#![forbid(unsafe_code)]\nuse forklaunch_ws::{codec::{PayloadCodec, PayloadError}, transport::{self, SocketSchemas, SocketEvent, SocketError, SocketOptions, IntoClientRequest, ClientHandshakeResponse}};\n",
    );
    source.push_str(&model_source(plan, &options.client_struct)?);
    let _ = writeln!(source, "pub const SOCKET_PATH: &str = {:?};", plan.path);
    let types: Vec<_> = payloads
        .iter()
        .map(|payload| payload.models.rust_type.as_str())
        .collect();
    let [incoming, outgoing, ping, pong, close] = types.as_slice() else {
        unreachable!()
    };
    let args = types.join(", ");
    let _ = writeln!(
        source,
        "pub type SocketConnection = transport::TypedWebSocket<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>, {args}>;"
    );
    let _ = writeln!(
        source,
        "pub struct {} {{ socket: SocketConnection }}",
        options.client_struct
    );
    let _ = writeln!(source, "impl {} {{", options.client_struct);
    let _ = writeln!(
        source,
        "pub fn schemas() -> Result<SocketSchemas<{args}>, PayloadError> {{"
    );
    for (name, payload) in ["incoming", "outgoing", "ping", "pong", "close"]
        .into_iter()
        .zip(payloads)
    {
        if payload.binary {
            let _ = writeln!(source, "let {name} = PayloadCodec::bytes();");
        } else {
            // A quoted JSON string is data, even when literals contain Rust delimiters.
            let json = serde_json::to_string(&payload.schema)?;
            let _ = writeln!(
                source,
                "let {name} = serde_json::from_str({json:?}).map_err(|error| PayloadError::Invalid(format!(\"embedded schema: {{error}}\")))?;"
            );
            let encoding = if payload.json_encoding {
                ".with_json_encoding()"
            } else {
                ""
            };
            let _ = writeln!(
                source,
                "let {name} = PayloadCodec::json_schema(&{name})?{encoding};"
            );
        }
    }
    source.push_str("Ok(SocketSchemas::new(incoming, outgoing)\n.with_ping(ping)\n.with_pong(pong)\n.with_close(close))\n}\n");
    source.push_str("pub async fn connect<R: IntoClientRequest + Unpin>(request: R, options: SocketOptions) -> Result<(Self, ClientHandshakeResponse), SocketError> {\nlet (socket, response) = transport::connect_with_response(request, Self::schemas()?, options).await?;\nOk((Self { socket }, response))\n}\n");
    source.push_str("pub fn from_socket(socket: SocketConnection) -> Self { Self { socket } }\npub fn socket(&mut self) -> &mut SocketConnection { &mut self.socket }\npub fn into_socket(self) -> SocketConnection { self.socket }\n");
    let _ = writeln!(
        source,
        "pub async fn send(&mut self, message: &{outgoing}) -> Result<(), SocketError> {{ self.socket.send(message).await }}"
    );
    let _ = writeln!(
        source,
        "pub async fn recv(&mut self) -> Result<Option<SocketEvent<{incoming}, {ping}, {pong}, {close}>>, SocketError> {{ self.socket.recv().await }}"
    );
    let _ = writeln!(
        source,
        "pub async fn ping(&mut self, value: Option<&{ping}>) -> Result<(), SocketError> {{ self.socket.ping(value).await }}"
    );
    let _ = writeln!(
        source,
        "pub async fn pong(&mut self, value: Option<&{pong}>) -> Result<(), SocketError> {{ self.socket.pong(value).await }}"
    );
    let _ = writeln!(
        source,
        "pub async fn close(&mut self, code: u16, reason: Option<&{close}>) -> Result<(), SocketError> {{ self.socket.close(code, reason).await }}\n}}"
    );
    Ok(source)
}

fn model_source(plan: &SocketSdkPlan, client_name: &str) -> Result<String> {
    let reserved = [
        "SocketConnection",
        "SocketSchemas",
        "SocketEvent",
        "SocketError",
        "SocketOptions",
        "PayloadCodec",
        "PayloadError",
        "Serialize",
        "Deserialize",
        "BTreeMap",
        "ClientHandshakeResponse",
        "IntoClientRequest",
        "transport",
        "serde",
        "serde_json",
        "tokio",
        "tokio_tungstenite",
        "forklaunch_ws",
        "std",
        "String",
        "Option",
        "Result",
        "Box",
        "Vec",
        "Default",
        "SOCKET_PATH",
        "bool",
        "char",
        "str",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "isize",
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "usize",
        "f32",
        "f64",
    ];
    let mut names: BTreeSet<_> = reserved.into_iter().map(str::to_owned).collect();
    if !names.insert(client_name.to_owned()) {
        bail!("Conflicting generated socket client name");
    }
    let mut models = Vec::new();
    let mut aliases = Vec::new();
    for payload in [
        &plan.incoming,
        &plan.outgoing,
        &plan.ping,
        &plan.pong,
        &plan.close,
    ] {
        for name in payload
            .models
            .rust_models
            .iter()
            .map(|model| &model.name)
            .chain(payload.models.rust_aliases.iter().map(|alias| &alias.name))
        {
            if !names.insert(name.clone()) {
                bail!("Conflicting generated socket type {name}");
            }
        }
        models.extend(payload.models.rust_models.clone());
        aliases.extend(payload.models.rust_aliases.clone());
    }
    render_rust_models(&models, &aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::ws::{SocketContract, SocketSchemaEntry};
    use serde_json::json;

    #[test]
    fn generated_socket_client_fixture() {
        let mut contract = SocketContract::new("/chat")
            .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"object","required":["message"],"properties":{"message":{"type":"string","minLength":1,"maxLength":32}},"additionalProperties":false})))
            .with_server_message("chat", SocketSchemaEntry::new(json!({"type":"object","required":["message"],"properties":{"message":{"type":"string","minLength":1,"maxLength":32}},"additionalProperties":false})));
        contract.ping = Some(SocketSchemaEntry::new(
            json!({"type":"object","required":["ts"],"properties":{"ts":{"type":"integer","minimum":1}},"additionalProperties":false}),
        ));
        contract.pong = contract.ping.clone();
        contract.close_reason.insert("reason".into(), SocketSchemaEntry::new(json!({"type":"object","required":["reason"],"properties":{"reason":{"type":"string"}},"additionalProperties":false})));
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let source =
            generate_rust_websocket_sdk_source(&plan, &RustSdkOptions::new("chat-sdk")).unwrap();
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_RUST_WS_SDK_OUTPUT") {
            std::fs::write(&output, source).unwrap();
            let recursive = SocketContract::new("/tree")
                .with_server_message("tree", SocketSchemaEntry::new(json!({"$ref":"#/$defs/Node%2525~1~0%20%E6%B0%B4", "$defs":{"Node%25/~ 水":{"type":"object","required":["value"],"properties":{"value":{"type":"integer"},"next":{"$ref":"#/$defs/Node%2525~1~0%20%E6%B0%B4"}},"additionalProperties":false}}})))
                .with_server_message("text", SocketSchemaEntry::new(json!({"type":"string"})))
                .with_client_message("accepted", SocketSchemaEntry::new(json!({"type":"boolean"})));
            for (suffix, contract) in [
                ("dictionary", SocketContract::new("/dictionary")
                    .with_server_message("dictionary", SocketSchemaEntry::new(json!({"type":"object","required":["item","const"],"properties":{"item":{"$ref":"#/properties"},"const":{"$ref":"#/definitions/Value"}},"definitions":{"Value":{"type":"string"}}})))
                    .with_server_message("number", SocketSchemaEntry::new(json!({"type":"integer"})))),
                ("recursive", recursive),
                ("empty", SocketContract::new("/empty")),
            ] {
                let source = generate_rust_websocket_sdk_source(
                    &SocketSdkPlan::from_contract(&contract).unwrap(),
                    &RustSdkOptions::new("fixture-sdk"),
                )
                .unwrap();
                std::fs::write(format!("{output}.{suffix}"), source).unwrap();
            }
        }
        for name in [
            "SocketIncoming",
            "SocketConnection",
            "transport",
            "serde_json",
            "bool",
            "i64",
            "Self",
            "Client; panic!()",
        ] {
            assert!(
                generate_rust_websocket_sdk_source(
                    &plan,
                    &RustSdkOptions::new("chat-sdk").with_client_struct(name)
                )
                .is_err(),
                "{name}"
            );
        }
    }
}
