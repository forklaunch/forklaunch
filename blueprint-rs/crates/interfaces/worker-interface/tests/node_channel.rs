#![cfg(all(feature = "schema", feature = "encrypted"))]
//! Uses the actual Notes DTO source, not a parallel approximation of its wire shape.
extern crate forklaunch_core as framework_core;
use forklaunch_core::contract;
extern crate forklaunch_worker_interface as worker;
#[path = "fixtures/note_events.rs"]
mod insight;
use forklaunch_core::build::{ChannelContextFields, ChannelEvent, ChannelWire, channel_events};
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use insight::{InsightEvent, NoteJob, PURPOSE, RESULT_PURPOSE, SealedNoteJob, TracedNoteJob};
use std::process::Command;
use worker::{EncryptedChannel, JobPayload};

fn context() -> worker::JobContext {
    worker::JobContext {
        tenant_id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
        operation_id: "22222222-2222-4222-8222-222222222222".parse().unwrap(),
        resource_id: "33333333-3333-4333-8333-333333333333".parse().unwrap(),
        revision: 1,
    }
}
fn key() -> EnvelopeKey {
    EnvelopeKey::with_rotation(
        EnvelopeProfile::new("notes").unwrap(),
        "previous".into(),
        [7; 32],
        vec![],
        None,
    )
    .unwrap()
}
fn node(source: &str) -> Vec<u8> {
    let out = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .output()
        .expect("Node required for channel interop");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
#[test]
#[allow(clippy::too_many_lines)] // Keep the complete cross-language security matrix in one isolated fixture.
fn actual_notes_channels_cross_language_rotation_tamper_schema_and_audit() {
    let context = context();
    let payload = NoteJob {
        operation_id: context.operation_id,
        workspace_id: context.tenant_id,
        note_id: context.resource_id,
        revision: 1,
        body: "sensitive note".into(),
    };
    let payload = worker::TracedJob::with_trace(
        payload,
        worker::TraceCarrier::try_from(
            "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01".to_owned(),
        )
        .unwrap(),
    );
    let result = InsightEvent {
        operation_id: context.operation_id,
        workspace_id: context.tenant_id,
        note_id: context.resource_id,
        revision: 1,
        excerpt: "sensitive insight".into(),
        word_count: 2,
    };
    let fields = ChannelContextFields {
        tenant: "workspaceId",
        resource: "noteId",
        operation: "operationId",
        revision: "revision",
    };
    let traced_fields = ChannelContextFields {
        tenant: "payload.workspaceId",
        resource: "payload.noteId",
        operation: "payload.operationId",
        revision: "payload.revision",
    };
    let output = std::env::temp_dir().join(format!("notes-channel-{}", std::process::id()));
    channel_events(
        "notes",
        &[
            ChannelEvent {
                name: "NoteJob",
                purpose: PURPOSE,
                schema: contract::schema_value_for::<TracedNoteJob>(),
                context: traced_fields,
                wire: ChannelWire::Flat {
                    context: fields,
                    ciphertext: "payload",
                },
            },
            ChannelEvent {
                name: "InsightEvent",
                purpose: RESULT_PURPOSE,
                schema: contract::schema_value_for::<InsightEvent>(),
                context: fields,
                wire: ChannelWire::SealedEvent,
            },
        ],
        include_str!("fixtures/note_events.rs"),
        &["framework_core", "worker", "uuid", "zeroize"],
        &output,
    )
    .unwrap();
    // Optional explicit path makes the same emitted assets available to the TS consumer proof.
    if let Ok(path) = std::env::var("FORKLAUNCH_CHANNEL_ASSET_FIXTURE") {
        std::fs::create_dir_all(&path).unwrap();
        for entry in std::fs::read_dir(output.join("typescript")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(
                entry.path(),
                std::path::Path::new(&path).join(entry.file_name()),
            )
            .unwrap();
        }
        std::fs::copy(
            output.join("event-contract.json"),
            std::path::Path::new(&path).join("event-contract.json"),
        )
        .unwrap();
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("event-contract.json")).unwrap())
            .unwrap();
    let sealed_job = SealedNoteJob {
        operation_id: context.operation_id,
        workspace_id: context.tenant_id,
        note_id: context.resource_id,
        revision: 1,
        payload: worker::seal_job(&key(), PURPOSE, &payload).unwrap(),
    };
    let sealed_result = EncryptedChannel::new(RESULT_PURPOSE)
        .unwrap()
        .seal(&key(), context.tenant_id, &result)
        .unwrap();
    let source = format!(
        r"
import assert from 'node:assert/strict';
import {{createEncryptedChannelProducer,createEncryptedChannelConsumer}} from {:?};
const descriptors={};const expected={};const payload={};
const config={{namespace:'notes',tenantId:expected.tenantId,activeKey:{{id:'active',key:Buffer.alloc(32,8)}},previousKeys:[{{id:'previous',key:Buffer.alloc(32,7)}}]}};
const validate = value => typeof value?.payload?.body === 'string' && Object.keys(value.payload).length===5;
const validateResult = value => typeof value?.excerpt === 'string' && Number.isInteger(value?.wordCount);
let audited=0;const audit=async record=>{{assert(!JSON.stringify(record).includes('sensitive'));audited++;return true;}};
const consumer=createEncryptedChannelConsumer(descriptors[0],{{...config,actor:'worker',audit}},validate);
const envelope={};assert.equal((await consumer.consume(envelope,expected)).payload.body,'sensitive note');assert.equal(audited,1);
const resultConsumer=createEncryptedChannelConsumer(descriptors[1],{{...config,actor:'api',audit}},validateResult);
assert.equal((await resultConsumer.consume({},expected)).excerpt,'sensitive insight');
await assert.rejects(consumer.consume(envelope,{{...expected,revision:2}}));
await assert.rejects(consumer.consume({{...envelope,revision:2}},expected));
await assert.rejects(createEncryptedChannelConsumer(descriptors[0],{{...config,tenantId:'44444444-4444-4444-8444-444444444444',actor:'worker',audit}},validate).consume(envelope,expected));
const parts=envelope.payload.split('.');const bytes=Buffer.from(parts[3],'base64url');bytes[0]^=1;parts[3]=bytes.toString('base64url');await assert.rejects(consumer.consume({{...envelope,payload:parts.join('.')}},expected));
await assert.rejects(createEncryptedChannelConsumer(descriptors[0],{{...config,actor:'worker',audit}},()=>false).consume(envelope,expected));
for(const failure of [async()=>false,async()=>{{throw Error('private audit failure');}},async()=>new Promise(()=>{{}})]){{
  const blocked=createEncryptedChannelConsumer(descriptors[0],{{...config,actor:'worker',audit:failure,auditTimeoutMs:5}},validate);
  await assert.rejects(blocked.consume(envelope,expected),/Encrypted channel unavailable/);blocked.close();
}}
await assert.rejects(createEncryptedChannelConsumer({{...descriptors[0],purpose:'other-v1'}},{{...config,actor:'worker',audit}},validate).consume(envelope,expected));
await assert.rejects(createEncryptedChannelConsumer(descriptors[0],{{...config,actor:'worker',audit,maxPayloadBytes:5}},validate).consume(envelope,expected));
let published;
const producer=createEncryptedChannelProducer(descriptors[0],{{...config,activeKey:{{id:'previous',key:Buffer.alloc(32,7)}},previousKeys:[],publish:async value=>{{assert(!JSON.stringify(value).includes('sensitive'));published=value;}}}},validate);
// Validate the actual serialization, not getters/toJSON on the original DTO.
for(const replacement of [{{...payload,payload:{{...payload.payload,body:42}}}},{{...payload,payload:{{...payload.payload,revision:2}}}}]) {{
  const transformed={{...payload}};Object.defineProperty(transformed,'toJSON',{{value:()=>replacement}});
  await assert.rejects(producer.publish(transformed,expected));assert.equal(published,undefined);
}}
for(const actor of ['worker\nforged','worker\u0085forged','界'.repeat(100)]) {{
  assert.throws(()=>createEncryptedChannelConsumer(descriptors[0],{{...config,actor,audit}},validate),/Encrypted channel unavailable/);
}}
// Invalid later rotation entries must wipe already-derived active key material.
let wiped=0;const originalFill=Buffer.prototype.fill;
try {{
  Buffer.prototype.fill=function(value,...rest) {{if(value===0 && this.length===32)wiped++;return originalFill.call(this,value,...rest);}};
  assert.throws(()=>createEncryptedChannelConsumer(descriptors[0],{{...config,previousKeys:[{{id:'bad',key:Buffer.alloc(31,1)}}],actor:'worker',audit}},validate),/Encrypted channel unavailable/);
  assert.equal(wiped,1);
}} finally {{Buffer.prototype.fill=originalFill;}}
await assert.rejects(producer.publish({{...payload,payload:{{...payload.payload,body:42}}}},expected));
await assert.rejects(producer.publish(payload,{{...expected,revision:2}}));
await producer.publish(payload,expected);
let publishedResult;
const resultPayload={{...payload.payload,excerpt:'sensitive insight',wordCount:2}};delete resultPayload.body;
const resultProducer=createEncryptedChannelProducer(descriptors[1],{{...config,publish:async value=>{{publishedResult=value;}}}},validateResult);
await resultProducer.publish(resultPayload,expected);
console.log(JSON.stringify({{job:published,result:publishedResult}}));resultProducer.close();
producer.close();await assert.rejects(producer.publish(payload,expected));consumer.close();await assert.rejects(consumer.consume(envelope,expected));
",
        format!(
            "file://{}/../../../../framework-rs/crates/core/src/build/runtime/encrypted-channel.mjs",
            env!("CARGO_MANIFEST_DIR")
        ),
        manifest["channels"],
        serde_json::to_string(&context).unwrap(),
        serde_json::to_string(&payload).unwrap(),
        serde_json::to_string(&sealed_job).unwrap(),
        serde_json::to_string(&sealed_result).unwrap()
    );
    let from_node: serde_json::Value = serde_json::from_slice(&node(&source)).unwrap();
    let from_node_result: worker::SealedEvent =
        serde_json::from_value(from_node["result"].clone()).unwrap();
    let rotated = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("notes").unwrap(),
        "active".into(),
        [8; 32],
        vec![("previous".into(), [7; 32])],
        None,
    )
    .unwrap();
    let opened_result = EncryptedChannel::<InsightEvent>::new(RESULT_PURPOSE)
        .unwrap()
        .open(&rotated, context.tenant_id, &from_node_result)
        .unwrap();
    assert_eq!(opened_result.excerpt, "sensitive insight");
    assert_eq!(opened_result.word_count, 2);
    let from_node: SealedNoteJob = serde_json::from_value(from_node["job"].clone()).unwrap();
    let opened = worker::decode_job::<TracedNoteJob>(
        &key(),
        from_node.context(),
        PURPOSE,
        &from_node.payload,
    )
    .unwrap();
    assert_eq!(opened.body, "sensitive note");
    assert_eq!(opened.context(), context);
    assert!(!insight::QUEUE.is_empty());
    std::fs::remove_dir_all(output).unwrap();
}
