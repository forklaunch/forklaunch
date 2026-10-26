//! Shared side-effect-free compiler for raw WebSocket SDK artifacts.
use anyhow::{Context, Result, bail};
use forklaunch_core::asyncapi::{AsyncApiInfo, generate_raw_socket_asyncapi_json};
use forklaunch_core::codegen::websocket::SocketSdkPlan;
use forklaunch_core::rust_sdk::{RustSdkOptions, generate_rust_websocket_sdk_source};
use forklaunch_core::typescript::{
    TypeScriptWebSocketSdkOptions, generate_typescript_websocket_sdk_source,
};
use forklaunch_core::ws::SocketContractArtifact;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Compile socket contracts or an imported `AsyncAPI` document without filesystem
/// access, running a process, or starting the application. The output contains
/// `typescript` and `rust` source maps plus the canonical `asyncapi` document.
/// Requires the `build` and `asyncapi-import` features.
///
/// # Errors
/// Rejects unsupported targets, sources, schemas, conflicting declarations, inputs
/// over 16 MiB and serialized output over 64 MiB before returning any output.
pub fn socket_artifact_sources(input: &[u8], target: &str, source: &str) -> Result<Value> {
    if !matches!(source, "socket-contract" | "asyncapi")
        || !matches!(target, "typescript" | "rust" | "all")
    {
        bail!("Expected target (typescript/rust/all) and source (socket-contract/asyncapi)");
    }
    if input.len() > 16 * 1024 * 1024 {
        bail!("Socket artifact exceeds 16 MiB");
    }
    let document = (source == "asyncapi")
        .then(|| serde_json::from_slice::<Value>(input))
        .transpose()?;
    let artifact: SocketContractArtifact = if let Some(document) = &document {
        forklaunch_core::asyncapi::import_server_websocket_asyncapi(document)?
    } else {
        serde_json::from_slice(input)?
    };
    let mut output = compile(&artifact, target)?;
    if let Some(document) = document {
        output["asyncapi"] = document;
    }
    serde_json::to_writer(OutputLimit(0), &output)?;
    Ok(output)
}

struct OutputLimit(usize);
impl std::io::Write for OutputLimit {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.saturating_add(bytes.len()) > 64 * 1024 * 1024 {
            return Err(std::io::Error::other(
                "Socket compiler output exceeds 64 MiB",
            ));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn compile(artifact: &SocketContractArtifact, target: &str) -> Result<Value> {
    if artifact.format_version != 1
        || artifact.title.is_empty()
        || artifact.version.is_empty()
        || artifact.sockets.is_empty()
        || artifact.sockets.len() > 64
    {
        bail!("Socket artifacts require formatVersion 1, title, version and 1 through 64 sockets");
    }
    let mut typescript = BTreeMap::new();
    let mut rust = BTreeMap::new();
    let mut document = json!({"asyncapi":"3.0.0", "info":{"title":artifact.title,"version":artifact.version},"channels":{},"operations":{}});
    for (name, contract) in &artifact.sockets {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            bail!(
                "Socket keys must contain only lowercase ASCII letters, digits and underscores (1 through 128 bytes)"
            );
        }
        let plan = SocketSdkPlan::from_contract(contract)
            .with_context(|| format!("Invalid socket {name}"))?;
        let module = format!("socket_{name}");
        if matches!(target, "typescript" | "all") {
            typescript.insert(
                module.clone(),
                generate_typescript_websocket_sdk_source(
                    &plan,
                    &TypeScriptWebSocketSdkOptions::new("SocketClient"),
                )?,
            );
        }
        if matches!(target, "rust" | "all") {
            let options = RustSdkOptions::new("socket-sdk").with_client_struct("SocketClient");
            rust.insert(module, generate_rust_websocket_sdk_source(&plan, &options)?);
        }
        let socket_document: Value = serde_json::from_str(&generate_raw_socket_asyncapi_json(
            &AsyncApiInfo::new(&artifact.title, &artifact.version),
            contract,
        )?)?;
        for section in ["channels", "operations"] {
            for (key, value) in socket_document[section].as_object().into_iter().flatten() {
                if document[section]
                    .get(key)
                    .is_some_and(|existing| existing != value)
                {
                    bail!("Conflicting AsyncAPI {section} entry {key}");
                }
                document[section][key] = value.clone();
            }
        }
    }
    Ok(json!({"typescript":typescript,"rust":rust,"asyncapi":document}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::ws::{SocketContract, SocketSchemaEntry};
    #[test]
    fn multi_socket_outputs_share_checked_generators_and_detect_conflicts() {
        let artifact = SocketContractArtifact::new("Fixture", "1")
            .with_socket(
                "chat",
                SocketContract::new("/chat")
                    .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"string"}))),
            )
            .with_socket(
                "tree",
                SocketContract::new("/tree")
                    .with_server_message("tree", SocketSchemaEntry::new(json!({"type":"integer"}))),
            );
        let output = compile(&artifact, "all").unwrap();
        assert!(
            output["typescript"]["socket_chat"]
                .as_str()
                .unwrap()
                .contains("export class SocketClient")
        );
        assert!(
            output["rust"]["socket_tree"]
                .as_str()
                .unwrap()
                .contains("pub struct SocketClient")
        );
        assert_eq!(
            output["asyncapi"]["operations"].as_object().unwrap().len(),
            2
        );
        assert!(
            compile(&artifact, "rust").unwrap()["typescript"]
                .as_object()
                .unwrap()
                .is_empty()
        );
        let mut invalid = artifact.clone();
        invalid.format_version = 2;
        assert!(compile(&invalid, "all").is_err());
        invalid = artifact.clone();
        invalid
            .sockets
            .insert("../bad".into(), SocketContract::new("/bad"));
        assert!(compile(&invalid, "all").is_err());
        invalid = artifact;
        invalid.sockets.insert(
            "conflict".into(),
            SocketContract::new("/other")
                .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"boolean"}))),
        );
        assert!(compile(&invalid, "all").is_err());
    }

    #[test]
    fn socket_artifact_inputs_are_bounded_and_imported_document_is_preserved() {
        let artifact = SocketContractArtifact::new("Fixture", "1").with_socket(
            "chat",
            SocketContract::new("/chat")
                .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"string"}))),
        );
        let input = serde_json::to_vec(&artifact).unwrap();
        let compiled = socket_artifact_sources(&input, "all", "socket-contract").unwrap();
        assert!(compiled["typescript"]["socket_chat"].is_string());
        let document = json!({
            "asyncapi":"3.0.0", "info":{"title":"Imported", "version":"1", "description":"Keep authored import metadata"},
            "defaultContentType":"application/json",
            "servers":{"local":{"host":"localhost:3000","protocol":"ws"}},
            "channels":{"chat":{"address":"/chat","bindings":{"ws":{"method":"GET","bindingVersion":"0.1.0"}},
                "messages":{"request":{"payload":{"type":"string"}}}}},
            "operations":{"accept":{"action":"receive","channel":{"$ref":"#/channels/chat"},
                "messages":[{"$ref":"#/channels/chat/messages/request"}]}}
        });
        let imported =
            socket_artifact_sources(&serde_json::to_vec(&document).unwrap(), "all", "asyncapi")
                .unwrap();
        assert_eq!(imported["asyncapi"], document);
        let normalized =
            forklaunch_core::asyncapi::import_server_websocket_asyncapi(&document).unwrap();
        let normalized_sources = socket_artifact_sources(
            &serde_json::to_vec(&normalized).unwrap(),
            "all",
            "socket-contract",
        )
        .unwrap();
        assert_eq!(imported["typescript"], normalized_sources["typescript"]);
        assert_eq!(imported["rust"], normalized_sources["rust"]);
        assert!(socket_artifact_sources(&input, "shell", "socket-contract").is_err());
        assert!(socket_artifact_sources(&input, "all", "openapi").is_err());
        assert!(socket_artifact_sources(&[b'x'; 16], "all", "socket-contract").is_err());
        assert!(
            socket_artifact_sources(&vec![b' '; 16 * 1024 * 1024 + 1], "all", "socket-contract")
                .unwrap_err()
                .to_string()
                .contains("16 MiB")
        );
        let mut limit = OutputLimit(64 * 1024 * 1024);
        assert!(std::io::Write::write(&mut limit, b"x").is_err());
    }
}
