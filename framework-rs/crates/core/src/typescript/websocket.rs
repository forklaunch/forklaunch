//! Checked Node WebSocket clients with typed raw-schema payloads.
use std::{collections::BTreeSet, fmt::Write as _};

use anyhow::{Result, bail};
use forklaunch_core::codegen::websocket::SocketSdkPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptWebSocketSdkOptions {
    pub client_name: String,
}
impl TypeScriptWebSocketSdkOptions {
    #[must_use]
    pub fn new(client_name: impl Into<String>) -> Self {
        Self {
            client_name: client_name.into(),
        }
    }
}

/// Generate a Node `ws` client with AJV validation, bounded queues and owned iterators.
/// Consumers need ws 8.22, AJV 8, ajv-draft-04 1, ajv-formats 3 and Node/ws TypeScript declarations.
/// Client class names must begin with an ASCII uppercase letter.
///
/// # Errors
/// Rejects invalid identifiers, conflicting names, unsupported dialects and schema serialization failures.
pub fn generate_typescript_websocket_sdk_source(
    plan: &SocketSdkPlan,
    options: &TypeScriptWebSocketSdkOptions,
) -> Result<String> {
    generate_socket_source(plan, options, false)
}

/// Generate a browser-native WebSocket client using the same checked wire codecs.
/// Browser authentication uses cookies; control frames are managed by the browser.
/// # Errors
/// Returns the same contract and identifier errors as the Node generator.
pub fn generate_typescript_browser_websocket_sdk_source(
    plan: &SocketSdkPlan,
    options: &TypeScriptWebSocketSdkOptions,
) -> Result<String> {
    generate_socket_source(plan, options, true)
}

fn generate_socket_source(
    plan: &SocketSdkPlan,
    options: &TypeScriptWebSocketSdkOptions,
    browser: bool,
) -> Result<String> {
    let mut names = client_names(&options.client_name)?;
    let payloads = [
        &plan.incoming,
        &plan.outgoing,
        &plan.ping,
        &plan.pong,
        &plan.close,
    ];
    for payload in payloads {
        if let Some(dialect) = payload
            .schema
            .get("$schema")
            .and_then(serde_json::Value::as_str)
            && !dialect.contains("/draft-04/")
            && !dialect.contains("/draft-06/")
            && !dialect.contains("/draft-07/")
            && !dialect.contains("/draft/2019-09/")
            && !dialect.contains("/draft/2020-12/")
        {
            bail!("Unsupported TypeScript WebSocket JSON Schema dialect: {dialect}");
        }
    }
    let aliases = [
        "SocketIncoming",
        "SocketOutgoing",
        "SocketPing",
        "SocketPong",
        "SocketClose",
    ];
    let mut source = runtime_source(browser);
    if browser {
        source.insert_str(0, "// Generated ForkLaunch browser WebSocket client.\n");
    }
    for (alias, payload) in aliases.into_iter().zip(payloads) {
        if !names.insert(alias.into()) {
            bail!("Conflicting TypeScript socket payload name {alias}");
        }
        for (name, definition) in &payload.models.typescript_definitions {
            if !names.insert(name.clone()) {
                bail!("Conflicting TypeScript socket definition {name}");
            }
            let _ = writeln!(source, "export type {name} = {definition};");
        }
        let _ = writeln!(
            source,
            "export type {alias} = {};",
            payload.models.typescript_type
        );
    }
    let schemas = serde_json::json!({"incoming":plan.incoming.schema,"outgoing":plan.outgoing.schema,"ping":plan.ping.schema,"pong":plan.pong.schema,"close":plan.close.schema});
    let binary = serde_json::json!({"incoming":plan.incoming.binary,"outgoing":plan.outgoing.binary,"ping":plan.ping.binary,"pong":plan.pong.binary,"close":false});
    let _ = writeln!(source, "const generatedSocketBinary = {binary};");
    let json_encoding = serde_json::json!({"incoming":plan.incoming.json_encoding,"outgoing":plan.outgoing.json_encoding,"ping":plan.ping.json_encoding,"pong":plan.pong.json_encoding,"close":plan.close.json_encoding});
    let _ = writeln!(
        source,
        "const generatedSocketJsonEncoding = {json_encoding};"
    );
    let encoded = forklaunch_common::json_string(&schemas.to_string());
    let _ = writeln!(
        source,
        "const generatedSocketSchemas = JSON.parse({encoded}) as SocketSchemaSet;"
    );
    let _ = writeln!(
        source,
        "export const SOCKET_PATH = {};",
        forklaunch_common::json_string(&plan.path)
    );
    let _ = writeln!(
        source,
        "export class {} extends SocketRuntime<SocketIncoming, SocketOutgoing, SocketPing, SocketPong, SocketClose> {{",
        options.client_name
    );
    source.push_str("private constructor(url: string, options: SocketClientOptions) { super(url, generatedSocketSchemas, options, generatedSocketBinary, generatedSocketJsonEncoding); }\n");
    let _ = writeln!(
        source,
        "static async connect(url: string, options: SocketClientOptions = {{}}): Promise<{}> {{",
        options.client_name
    );
    let _ = writeln!(
        source,
        "const client = new {}(url, options); await client.waitForOpen(); return client;\n}}\n}}",
        options.client_name
    );
    Ok(source)
}

fn runtime_source(browser: bool) -> String {
    format!(
        "{}\n{}\n{}",
        crate::typescript::typescript_schema_runtime_source(),
        include_str!("websocket-codec.ts"),
        if browser {
            include_str!("websocket-browser-runtime.ts")
        } else {
            include_str!("websocket-runtime.ts")
        }
    )
}

fn client_names(client_name: &str) -> Result<BTreeSet<String>> {
    let mut chars = client_name.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_uppercase())
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        bail!("Unsupported TypeScript socket client identifier");
    }
    let reserved = [
        "WebSocket",
        "URL",
        "TextEncoder",
        "TextDecoder",
        "ArrayBuffer",
        "Date",
        "TypeError",
        "SocketRuntime",
        "SocketCodec",
        "PortableSocketCodec",
        "SocketEvent",
        "SocketClientOptions",
        "SocketReadOptions",
        "SocketWriteOptions",
        "SocketUpgradeResponse",
        "SocketSchemaSet",
        "SocketWaiter",
        "ClientOptions",
        "RawData",
        "IncomingHttpHeaders",
        "Ajv",
        "Ajv2020",
        "Ajv2019",
        "AnySchema",
        "ValidateFunction",
        "Record",
        "Buffer",
        "Uint8Array",
        "Partial",
        "Error",
        "Promise",
        "Set",
        "WeakSet",
        "Object",
        "Number",
        "String",
        "BigInt",
        "Array",
        "Math",
        "JSON",
        "Symbol",
        "AbortSignal",
        "AsyncIterator",
        "AsyncIterable",
        "ReturnType",
        "Pick",
        "Omit",
        "SOCKET_PATH",
    ];
    let mut names: BTreeSet<_> = reserved.into_iter().map(str::to_owned).collect();
    if !names.insert(client_name.to_owned()) {
        bail!("Conflicting TypeScript socket client name");
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::ws::{SocketContract, SocketSchemaEntry};
    use serde_json::json;

    #[test]
    fn generated_typescript_socket_client_fixture() {
        let mut contract = SocketContract::new("/chat")
            .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"object","required":["message"],"properties":{"message":{"type":"string","minLength":1,"maxLength":32}},"additionalProperties":false})))
            .with_server_message("chat", SocketSchemaEntry::new(json!({"type":"object","required":["message"],"properties":{"message":{"type":"string","minLength":1,"maxLength":32}},"additionalProperties":false})));
        contract.ping = Some(SocketSchemaEntry::new(
            json!({"type":"object","required":["ts"],"properties":{"ts":{"type":"integer","minimum":1}},"additionalProperties":false}),
        ));
        contract.pong = contract.ping.clone();
        contract.close_reason.insert("reason".into(), SocketSchemaEntry::new(json!({"type":"object","required":["reason"],"properties":{"reason":{"type":"string"}},"additionalProperties":false})));
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let source = generate_typescript_websocket_sdk_source(
            &plan,
            &TypeScriptWebSocketSdkOptions::new("ForkLaunchSocketClient"),
        )
        .unwrap();
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_TS_WS_SDK_OUTPUT") {
            std::fs::write(&output, source).unwrap();
            let browser = generate_typescript_browser_websocket_sdk_source(
                &plan,
                &TypeScriptWebSocketSdkOptions::new("ForkLaunchSocketClient"),
            )
            .unwrap();
            assert!(!browser.contains("from 'ws'"));
            assert!(!browser.contains("Buffer."));
            std::fs::write(format!("{output}.browser.ts"), browser).unwrap();
            let dates = SocketSchemaEntry::new(
                json!({"type":"object","required":["at"],"properties":{"at":{"type":"string","format":"date-time","x-forklaunch-date-codec":{"version":1,"kind":"utc-milliseconds","projection":"value"}}},"additionalProperties":false}),
            );
            let date_contract = SocketContract::new("/dates")
                .with_client_message("date", dates.clone())
                .with_server_message("date", dates);
            let date_source = generate_typescript_browser_websocket_sdk_source(
                &SocketSdkPlan::from_contract(&date_contract).unwrap(),
                &TypeScriptWebSocketSdkOptions::new("DateSocketClient"),
            )
            .unwrap();
            std::fs::write(format!("{output}.date.browser.ts"), date_source).unwrap();

            contract.server_messages.clear();
            contract.server_messages.insert("reply".into(), SocketSchemaEntry::new(json!({"type":"object","required":["accepted"],"properties":{"accepted":{"type":"string"}},"additionalProperties":false})));
            let native = generate_typescript_websocket_sdk_source(
                &SocketSdkPlan::from_contract(&contract).unwrap(),
                &TypeScriptWebSocketSdkOptions::new("ForkLaunchSocketClient"),
            )
            .unwrap();
            std::fs::write(format!("{output}.native"), native).unwrap();
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
                ("draft4", SocketContract::new("/draft-four")
                    .with_server_message("legacy", SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"string","const":"ignored"})))),
                ("draft6", SocketContract::new("/draft-six")
                    .with_server_message("values", SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-06/schema#","type":"array","items":{"type":"integer"},"contains":{"minimum":2}})))
                    .with_client_message("request", SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-06/schema#","type":"string","minLength":1})))),
            ] {
                let source = generate_typescript_websocket_sdk_source(
                    &SocketSdkPlan::from_contract(&contract).unwrap(),
                    &TypeScriptWebSocketSdkOptions::new("ForkLaunchSocketClient"),
                )
                .unwrap();
                std::fs::write(format!("{output}.{suffix}"), source).unwrap();
            }
        }
        for name in [
            "SocketIncoming",
            "WebSocket",
            "TextEncoder",
            "Number",
            "x; throw Error()",
        ] {
            assert!(
                generate_typescript_websocket_sdk_source(
                    &plan,
                    &TypeScriptWebSocketSdkOptions::new(name)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn generates_declared_draft_four_clients_without_newer_literal_constraints() {
        let contract = SocketContract::new("/legacy").with_server_message(
            "legacy", SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"string","const":"ignored"})),
        );
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        assert_eq!(plan.incoming.models.typescript_type, "string");
        let source = generate_typescript_websocket_sdk_source(
            &plan,
            &TypeScriptWebSocketSdkOptions::new("LegacyClient"),
        )
        .unwrap();
        assert!(source.contains("ajv-draft-04"));
    }
    #[test]
    fn generates_declared_draft_six_clients() {
        let contract = SocketContract::new("/draft-six").with_server_message(
            "values", SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-06/schema#","type":"array","items":{"type":"integer"},"contains":{"minimum":2}})),
        );
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let source = generate_typescript_websocket_sdk_source(
            &plan,
            &TypeScriptWebSocketSdkOptions::new("DraftSixClient"),
        )
        .unwrap();
        assert!(source.contains("const socketDraft6MetaSchema = JSON.parse("));
        assert!(source.contains("export type SocketIncoming = Array<number>"));
    }
}
