//! Explicit native encrypted-channel DTO and wire contracts, independent of queues.
use crate::build::write_output;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fmt::Write as _, path::Path};

/// JSON member names mapping an application DTO to authenticated job context.
#[derive(Clone, Copy)]
pub struct ChannelContextFields<'a> {
    pub tenant: &'a str,
    pub resource: &'a str,
    pub operation: &'a str,
    pub revision: &'a str,
}
impl ChannelContextFields<'_> {
    fn json(self) -> Value {
        let path = |name: &str| {
            if name.contains('.') {
                json!(name.split('.').collect::<Vec<_>>())
            } else {
                json!(name)
            }
        };
        json!({"tenantId":path(self.tenant),"resourceId":path(self.resource),"operationId":path(self.operation),"revision":path(self.revision)})
    }
    fn names(self) -> [String; 4] {
        [self.tenant, self.resource, self.operation, self.revision].map(str::to_owned)
    }
}
/// Envelopes are explicit: no implicit work-v1 wrapper is introduced.
pub enum ChannelWire<'a> {
    SealedEvent,
    Flat {
        context: ChannelContextFields<'a>,
        ciphertext: &'a str,
    },
}
/// One DTO, one authenticated encryption purpose, and its actual broker wire shape.
pub struct ChannelEvent<'a> {
    pub name: &'a str,
    pub purpose: &'a str,
    pub schema: Value,
    pub context: ChannelContextFields<'a>,
    pub wire: ChannelWire<'a>,
}
fn unique(names: &[String]) -> bool {
    names
        .iter()
        .all(|s| !s.is_empty() && s != "__proto__" && s != "constructor" && s != "prototype")
        && names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == names.len()
}
// Only required object members through bounded local schema references qualify.
// Optional, nullable, union and external-reference paths fail closed.
fn required_path(root: &Value, path: &str) -> bool {
    if path.split('.').count() > 8 {
        return false;
    }
    let mut schema = root;
    let mut references = 0;
    for member in path.split('.') {
        if member.is_empty() || matches!(member, "__proto__" | "constructor" | "prototype") {
            return false;
        }
        while let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            references += 1;
            if references > 32 {
                return false;
            }
            let Some(pointer) = reference.strip_prefix('#') else {
                return false;
            };
            let Some(resolved) = root.pointer(pointer) else {
                return false;
            };
            schema = resolved;
        }
        if schema.get("type").and_then(Value::as_str) != Some("object")
            || !schema
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|value| value.as_str() == Some(member)))
        {
            return false;
        }
        let Some(property) = schema
            .get("properties")
            .and_then(|properties| properties.get(member))
        else {
            return false;
        };
        schema = property;
    }
    true
}

fn descriptor(event: &ChannelEvent<'_>) -> Result<Value> {
    ensure!(
        event
            .name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            && event.name.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid or duplicate channel type name"
    );
    ensure!(
        !event.purpose.is_empty()
            && event.purpose.len() <= 64
            && event
                .purpose
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid or duplicate purpose"
    );
    ensure!(
        unique(&event.context.names()),
        "ambiguous payload context fields"
    );
    for field in event.context.names() {
        ensure!(
            required_path(&event.schema, &field),
            "missing required context field"
        );
    }
    let wire = match event.wire {
        ChannelWire::SealedEvent => json!({"kind":"sealed-event"}),
        ChannelWire::Flat {
            context,
            ciphertext,
        } => {
            let mut fields = context.names().to_vec();
            fields.push(ciphertext.into());
            ensure!(
                unique(&fields) && fields.iter().all(|field| !field.contains('.')),
                "ambiguous envelope fields"
            );
            json!({"kind":"flat","contextFields":context.json(),"ciphertextField":ciphertext})
        }
    };
    let descriptor = json!({"name":event.name,"purpose":event.purpose,"schema":event.schema,"contextFields":event.context.json(),"wire":wire});
    Ok(descriptor)
}

/// Emit typed encrypted producers and audited consumers from host-compiled DTO schemas.
/// Rust output is the original shared source, preserving its JobPayload/Zeroize logic;
/// callers declare its crate dependencies in `rust_dependencies` (not a standalone crate).
/// # Errors
/// Rejects ambiguous context/wire mappings, invalid names, unsupported schemas or I/O.
pub fn channel_events(
    service: &str,
    events: &[ChannelEvent<'_>],
    rust_source: &str,
    rust_dependencies: &[&str],
    output: impl AsRef<Path>,
) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    let mut purposes = std::collections::BTreeSet::new();
    let mut descriptors = Vec::new();
    let mut ts = String::from(
        "// Generated from actual Rust encrypted-channel DTOs.\nimport {Ajv, type Plugin} from 'ajv';\nimport addFormats from 'ajv-formats';\nimport {createEncryptedChannelProducer, createEncryptedChannelConsumer} from './encrypted-channel.mjs';\nimport type {ProducerOptions, ConsumerOptions, ChannelContext} from './encrypted-channel.mjs';\nexport type {ProducerOptions, ConsumerOptions, ChannelContext, AuditRecord} from './encrypted-channel.mjs';\nconst ajv = new Ajv({strict: true, allErrors: false});\n// ajv-formats publishes a CommonJS callable with a default export.
(addFormats as unknown as Plugin<unknown>)(ajv);
// Rust integer annotations remain enforced within JavaScript's exact integer range.
for (const bits of [8,16,32,64]) {
  for (const unsigned of [false,true]) {
    const max = Math.min(Number.MAX_SAFE_INTEGER, 2 ** (unsigned ? bits : bits-1) - 1);
    const min = unsigned ? 0 : Math.max(Number.MIN_SAFE_INTEGER, -(2 ** (bits-1)));
    ajv.addFormat(`${unsigned ? 'uint' : 'int'}${bits}`, {type: 'number', validate: (value:number) => Number.isSafeInteger(value) && value >= min && value <= max});
  }
}
\n",
    );
    for event in events {
        ensure!(
            names.insert(event.name) && purposes.insert(event.purpose),
            "duplicate channel name or purpose"
        );
        let descriptor = descriptor(event)?;
        let model = forklaunch_core::codegen::compile_schema(event.name, &event.schema)?;
        for (name, value) in model.typescript_definitions {
            writeln!(&mut ts, "export type {name} = {value};")?;
        }
        writeln!(
            &mut ts,
            "export type {} = {};",
            event.name, model.typescript_type
        )?;
        write!(
            &mut ts,
            "const {}Descriptor = {} as const;\nconst validate{} = ajv.compile<{}>({}Descriptor.schema);\n",
            event.name, descriptor, event.name, event.name, event.name
        )?;
        write!(
            &mut ts,
            "export function create{}Producer(options: ProducerOptions) {{ return createEncryptedChannelProducer({}Descriptor, options, validate{}); }}\nexport function create{}Consumer(options: ConsumerOptions) {{ return createEncryptedChannelConsumer({}Descriptor, options, validate{}); }}\n",
            event.name, event.name, event.name, event.name, event.name, event.name
        )?;
        descriptors.push(descriptor);
    }
    let manifest = json!({"format":"forklaunch.encrypted-channels.v1","service":service,"protocol":"native-v3-hkdf-sha256-aes-256-gcm","channels":descriptors,"rust":{"kind":"shared-source-module","dependencies":rust_dependencies},"consumerAudit":"durable-metadata-acknowledgment-before-plaintext-release"});
    let output = output.as_ref();
    for (name, contents) in [
        (
            "event-contract.json",
            serde_json::to_string_pretty(&manifest)?,
        ),
        ("typescript/events.ts", ts),
        (
            "typescript/encrypted-channel.mjs",
            include_str!("runtime/encrypted-channel.mjs").into(),
        ),
        (
            "typescript/encrypted-channel.d.mts",
            include_str!("runtime/encrypted-channel.d.mts").into(),
        ),
        (
            "typescript/package.json",
            crate::build::service_package::event_package(service)?,
        ),
        (
            "typescript/tsconfig.json",
            crate::build::service_package::EVENT_TSCONFIG.into(),
        ),
        ("rust/events.rs", rust_source.into()),
        (
            "rust/README.md",
            format!(
                "Shared source module compiled by the service; not a standalone crate.\nRequired dependencies/crate aliases: {}.\n",
                rust_dependencies.join(", ")
            ),
        ),
    ] {
        write_output(&output.join(name), &contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod context_path_tests {
    use super::*;
    #[test]
    fn nested_context_requires_every_member_and_resolves_only_local_refs() {
        let mut schema = json!({"type":"object","required":["payload"],"properties":{"payload":{"$ref":"#/$defs/Job"}},"$defs":{"Job":{"type":"object","required":["tenant"],"properties":{"tenant":{"type":"string"}}}}});
        assert!(required_path(&schema, "payload.tenant"));
        let fields = ChannelContextFields {
            tenant: "payload.tenant",
            resource: "payload.resource",
            operation: "payload.operation",
            revision: "payload.revision",
        };
        assert_eq!(fields.json()["tenantId"], json!(["payload", "tenant"]));
        assert!(!required_path(&schema, "payload.missing"));
        assert!(!required_path(&schema, "payload.__proto__"));
        schema["$defs"]["Job"]["required"] = json!([]);
        assert!(!required_path(&schema, "payload.tenant"));
        schema["properties"]["payload"]["$ref"] = json!("https://example.invalid/schema");
        assert!(!required_path(&schema, "payload.tenant"));
        schema["properties"]["payload"]["$ref"] = json!("#/properties/payload");
        assert!(!required_path(&schema, "payload.tenant"));
    }
}
