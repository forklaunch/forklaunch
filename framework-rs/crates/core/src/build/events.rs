//! Worker event assets, separate from the HTTP route registry.
use crate::build::write_output;
use anyhow::Result;
use serde_json::Value;
use std::{fmt::Write as _, path::Path};

/// Emit a portable event manifest, TypeScript producer contract, and the same
/// Rust declarations plus a framework-backed encrypted producer helper. Consumers
/// of that Rust module enable core `typed-schema` and `encryption`, worker-interface
/// `schema`, UUID and zeroize. The TypeScript transport must
/// implement authenticated encryption; this module never publishes plaintext.
///
/// # Errors
/// Rejects schemas unsupported by the shared compiler and filesystem failures.
pub fn worker_events(
    worker: &str,
    input: &Value,
    event: &Value,
    rust_source: &str,
    output: impl AsRef<Path>,
) -> Result<()> {
    let output = output.as_ref();
    let mut ts = String::from("// Generated from the worker's Rust event contract.\n");
    ts.push_str(&typescript_declarations(input, event)?);
    write!(
        &mut ts,
        "const inputSchema = {};\nconst eventSchema = {};\n",
        serde_json::to_string(input)?,
        serde_json::to_string(event)?
    )?;
    ts.push_str(r#"
import {Ajv, type Plugin} from 'ajv';
import addFormats from 'ajv-formats';
import {createAuditedWorkConsumer} from './encrypted-worker.mjs';
import type {ConsumerOptions, ChannelContext} from './encrypted-channel.mjs';
export type {ConsumerOptions, ChannelContext, AuditRecord, ChannelKey} from './encrypted-channel.mjs';
export {createEncryptedWorkPublisher} from './encrypted-worker.mjs';
const validator = new Ajv({strict: true, allErrors: false});
(addFormats as unknown as Plugin<unknown>)(validator);
for (const bits of [8,16,32,64]) {
  for (const unsigned of [false,true]) {
    const max = Math.min(Number.MAX_SAFE_INTEGER, 2 ** (unsigned ? bits : bits-1) - 1);
    const min = unsigned ? 0 : Math.max(Number.MIN_SAFE_INTEGER, -(2 ** (bits-1)));
    validator.addFormat(`${unsigned ? 'uint' : 'int'}${bits}`, {type:'number', validate:(value:number)=>Number.isSafeInteger(value) && value >= min && value <= max});
  }
}
const validateInput = validator.compile<WorkInput>(inputSchema);
const validateEvent = validator.compile<WorkEvent>(eventSchema);
/** Adapter must seal with the configured trusted tenant, key and work-v1 channel,
 * bind queue and operation identity, and publish only the resulting SealedEvent.
 * A raw broker client does not implement this interface. */
export interface EncryptedWorkPublisher {
  sealAndPublish(request: { readonly channel: "work-v1"; readonly queue: string;
    readonly operationId: string; readonly input: WorkInput }): Promise<void>;
}
export class WorkProducer {
  constructor(private readonly publisher: EncryptedWorkPublisher, private readonly queue: string) {}
  async publish(operationId: string, input: WorkInput): Promise<void> {
    let resourceId:unknown, revision:unknown, snapshot:unknown;
    try {
      resourceId=input.resourceId; revision=input.revision;
      const serialized=JSON.stringify(input);
      if (new TextEncoder().encode(serialized).length > 1048576) throw new Error();
      snapshot=JSON.parse(serialized);
    } catch { throw new Error("Worker event contract rejected"); }
    if (!validateInput(snapshot) || snapshot.resourceId !== resourceId || snapshot.revision !== revision) throw new Error("Worker event contract rejected");
    await this.publisher.sealAndPublish({channel: "work-v1", queue: this.queue, operationId, input:snapshot});
  }
}
/** The trusted context must come from authorization/dispatch state, not the envelope.
 * Audit must acknowledge durable metadata-only storage before plaintext is returned. */
export class WorkConsumer {
  private readonly consumer: ReturnType<typeof createAuditedWorkConsumer<WorkEvent>>;
  constructor(options: ConsumerOptions, queue: string) {
    this.consumer=createAuditedWorkConsumer(options,queue,validateEvent);
  }
  consume(expected: ChannelContext, envelope: unknown): Promise<WorkEvent> {
    return this.consumer.consume(envelope,expected);
  }
  close():void {this.consumer.close();}
}
"#);
    let manifest = serde_json::json!({
        "format": "forklaunch.worker-events.v1", "worker": worker,
        "channel": "work-v1", "transport": "authenticated-encrypted-event",
        "input": input, "event": event,
        "typescriptTransport": "Node crypto HKDF-SHA256 AES-256-GCM v3",
        "consumerAudit": "durable-metadata-acknowledgment-before-plaintext-release"
    });
    write_output(
        &output.join("event-contract.json"),
        &serde_json::to_string_pretty(&manifest)?,
    )?;
    write_output(&output.join("typescript/events.ts"), &ts)?;
    write_output(
        &output.join("typescript/package.json"),
        &crate::build::service_package::event_package(worker)?,
    )?;
    write_output(
        &output.join("typescript/tsconfig.json"),
        crate::build::service_package::EVENT_TSCONFIG,
    )?;
    write_output(
        &output.join("typescript/encrypted-worker.mjs"),
        include_str!("runtime/encrypted-worker.mjs"),
    )?;
    write_output(
        &output.join("typescript/encrypted-worker.d.mts"),
        include_str!("runtime/encrypted-worker.d.mts"),
    )?;
    write_output(
        &output.join("typescript/encrypted-channel.mjs"),
        include_str!("runtime/encrypted-channel.mjs"),
    )?;
    write_output(
        &output.join("typescript/encrypted-channel.d.mts"),
        include_str!("runtime/encrypted-channel.d.mts"),
    )?;
    write_output(
        &output.join("rust/events.rs"),
        &format!(
            "{rust_source}\n{}",
            include_str!("runtime/work_producer.rs")
        ),
    )
}

fn typescript_declarations(input: &Value, event: &Value) -> Result<String> {
    let mut ts = String::new();
    crate::codegen::validate_schema_identities([input, event])?;
    let mut definitions = std::collections::BTreeMap::new();
    for (name, schema) in [("WorkInput", input), ("WorkEvent", event)] {
        let model = forklaunch_core::codegen::compile_schema(name, schema)?;
        for (key, value) in model.typescript_definitions.into_iter().chain(
            (model.typescript_type != name).then_some((name.to_owned(), model.typescript_type)),
        ) {
            anyhow::ensure!(
                definitions
                    .get(&key)
                    .is_none_or(|previous| previous == &value),
                "Conflicting worker event declaration {key}"
            );
            definitions.insert(key, value);
        }
    }
    for (name, definition) in definitions {
        writeln!(&mut ts, "export type {name} = {definition};")?;
    }
    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_input_graph_is_shared_and_conflicting_assertions_are_rejected() {
        let path = std::env::temp_dir().join(format!("worker-shared-model-{}", std::process::id()));
        let input = serde_json::json!({
            "$schema":"http://json-schema.org/draft-07/schema#",
            "type":"object", "additionalProperties":false,
            "x-forklaunch-model":{"id":"jobs::WorkInput","name":"WorkInput","projection":"value","version":1},
            "required":["resourceId","revision","message"],
            "properties":{"resourceId":{"type":"string"},"revision":{"type":"integer"},
                "message":{"type":"string","minLength":1}}
        });
        let mut nested = input.clone();
        nested.as_object_mut().unwrap().remove("$schema");
        let mut event = serde_json::json!({
            "$schema":"http://json-schema.org/draft-07/schema#",
            "type":"object", "required":["input"],
            "definitions":{"WorkInput":nested},
            "properties":{"input":{"$ref":"#/definitions/WorkInput"}}
        });
        worker_events("jobs", &input, &event, "pub struct WorkInput;", &path).unwrap();
        let source = std::fs::read_to_string(path.join("typescript/events.ts")).unwrap();
        assert_eq!(source.matches("export type WorkInput =").count(), 1);
        if let Some(output) = std::env::var_os("FORKLAUNCH_WORKER_MODEL_FIXTURE") {
            worker_events("jobs", &input, &event, "pub struct WorkInput;", output).unwrap();
        }
        event["definitions"]["WorkInput"]["properties"]["message"]["minLength"] = 2.into();
        let error = worker_events("jobs", &input, &event, "", &path).unwrap_err();
        assert!(error.to_string().contains("Conflicting schemas"));
        assert_eq!(
            std::fs::read_to_string(path.join("typescript/events.ts")).unwrap(),
            source
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn event_assets_share_schema_and_recover_deleted_output() {
        let path = std::env::temp_dir().join(format!("forklaunch-events-{}", std::process::id()));
        let input = serde_json::json!({"type":"object","additionalProperties":false,
            "required":["resourceId","revision","message"],"properties":{
            "resourceId":{"type":"string","format":"uuid"},"revision":{"type":"integer"},"message":{"type":"string"}}});
        let event = serde_json::json!({"type":"object","required":["input"],"properties":{"input":input.clone()}});
        worker_events("jobs", &input, &event, "pub struct WorkInput;", &path).unwrap();
        assert!(
            std::fs::read_to_string(path.join("rust/events.rs"))
                .unwrap()
                .contains("pub fn seal_work")
        );
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(path.join("event-contract.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["input"], input);
        assert_eq!(manifest["event"], event);
        let ts = std::fs::read_to_string(path.join("typescript/events.ts")).unwrap();
        assert!(ts.contains("WorkInput"));
        assert!(ts.contains("sealAndPublish"));
        assert!(ts.contains("revision: number") || ts.contains("\"revision\": number"));
        std::fs::remove_file(path.join("typescript/events.ts")).unwrap();
        worker_events("jobs", &input, &event, "pub struct WorkInput;", &path).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.join("typescript/events.ts")).unwrap(),
            ts
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
