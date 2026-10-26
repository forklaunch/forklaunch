#![cfg(all(feature = "schema", feature = "encrypted"))]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use forklaunch_worker_interface::{EncryptedChannel, JobContext, JobPayload, SealedEvent};
use std::process::Command;
use zeroize::Zeroize;

#[forklaunch_core::contract::schema]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    resource_id: uuid::Uuid,
    revision: i64,
    message: String,
}
#[forklaunch_core::contract::schema]
#[serde(deny_unknown_fields)]
struct Event {
    context: JobContext,
    queue: String,
    input: Input,
}
impl Zeroize for Event {
    fn zeroize(&mut self) {
        self.input.message.zeroize();
    }
}
impl JobPayload for Event {
    fn context(&self) -> JobContext {
        self.context
    }
}

#[test]
fn node_producer_encrypts_before_adapter_and_rust_authenticates() {
    if let Ok(path) = std::env::var("FORKLAUNCH_WORK_ASSET_FIXTURE") {
        forklaunch_core::build::worker_events(
            "tasks",
            &forklaunch_core::contract::schema_value_for::<Input>(),
            &forklaunch_core::contract::schema_value_for::<Event>(),
            "// Test DTO source",
            std::path::Path::new(&path).join("events"),
        )
        .unwrap();
    }
    let source = format!(
        r"
import {{createEncryptedWorkPublisher}} from {:?};
const producer = createEncryptedWorkPublisher({{namespace:'jobs', tenantId:'11111111-1111-4111-8111-111111111111',
  keyId:'primary', key:Buffer.alloc(32,7), async publish(event) {{
    if (JSON.stringify(event).includes('private marker')) throw Error('plaintext leak');
    console.log(JSON.stringify(event));
  }} }});
await producer.sealAndPublish({{channel:'work-v1',queue:'jobs',operationId:'22222222-2222-4222-8222-222222222222',
 input:{{resourceId:'33333333-3333-4333-8333-333333333333',revision:1,message:'private marker'}} }});
producer.close();
let rejected = false;
try {{await producer.sealAndPublish({{channel:'work-v1',queue:'jobs',operationId:'22222222-2222-4222-8222-222222222222', input:{{}}}});}} catch {{ rejected = true; }}
if (!rejected) throw Error('disposed key accepted');
",
        format!(
            "file://{}/../../../../framework-rs/crates/core/src/build/runtime/encrypted-worker.mjs",
            env!("CARGO_MANIFEST_DIR")
        )
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &source])
        .output()
        .expect("Node is required for cross-language worker verification");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sealed: SealedEvent = serde_json::from_slice(&out.stdout).unwrap();
    let key = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("jobs").unwrap(),
        "primary".into(),
        [7; 32],
        vec![],
        None,
    )
    .unwrap();
    let channel = EncryptedChannel::<Event>::new("work-v1").unwrap();
    let tenant = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let opened = channel.open(&key, tenant, &sealed).unwrap();
    assert_eq!(opened.queue, "jobs");
    assert_eq!(opened.input.message, "private marker");
    assert_eq!(opened.input.resource_id, opened.context.resource_id);
    assert_eq!(opened.input.revision, opened.context.revision);
    assert!(
        channel
            .open(
                &key,
                "44444444-4444-4444-8444-444444444444".parse().unwrap(),
                &sealed
            )
            .is_err()
    );
    let mut changed = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap();
    changed["context"]["revision"] = 2.into();
    assert!(
        channel
            .open(&key, tenant, &serde_json::from_value(changed).unwrap())
            .is_err()
    );
}

#[test]
fn rust_producer_node_consumer_rejects_metadata_ciphertext_and_purpose_tampering() {
    let tenant = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let operation = "22222222-2222-4222-8222-222222222222".parse().unwrap();
    let resource = "33333333-3333-4333-8333-333333333333".parse().unwrap();
    let key = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("jobs").unwrap(),
        "primary".into(),
        [7; 32],
        vec![],
        None,
    )
    .unwrap();
    let event = Event {
        context: JobContext {
            tenant_id: tenant,
            operation_id: operation,
            resource_id: resource,
            revision: 1,
        },
        queue: "jobs".into(),
        input: Input {
            resource_id: resource,
            revision: 1,
            message: "private marker".into(),
        },
    };
    let sealed = EncryptedChannel::new("work-v1")
        .unwrap()
        .seal(&key, tenant, &event)
        .unwrap();
    let wrong_purpose = EncryptedChannel::new("other-v1")
        .unwrap()
        .seal(&key, tenant, &event)
        .unwrap();
    let source = format!(
        r"
import {{createEncryptedWorkConsumer,createAuditedWorkConsumer}} from {:?};
const config={{namespace:'jobs',tenantId:'11111111-1111-4111-8111-111111111111',keyId:'primary',key:Buffer.alloc(32,7)}};
const consumer=createEncryptedWorkConsumer(config);
const event={};
const trusted={{queue:'jobs',operationId:'22222222-2222-4222-8222-222222222222'}};
if(consumer.open(event,trusted).input.message !== 'private marker') throw Error('reverse decrypt failed');
const reject=(fn)=>{{let failed=false;try{{fn();}}catch{{failed=true;}}if(!failed)throw Error('tamper accepted');}};
reject(()=>consumer.open(event,{{...trusted,queue:'other'}}));
reject(()=>consumer.open(event,{{...trusted,operationId:'44444444-4444-4444-8444-444444444444'}}));
reject(()=>createEncryptedWorkConsumer({{...config,tenantId:'44444444-4444-4444-8444-444444444444'}}).open(event,trusted));
reject(()=>createEncryptedWorkConsumer({{...config,namespace:'other'}}).open(event,trusted));
reject(()=>consumer.open({{...event,context:{{...event.context,revision:2}}}},trusted));
const purpose={};purpose.channel='work-v1';reject(()=>consumer.open(purpose,trusted));
const parts=event.ciphertext.split('.');const bytes=Buffer.from(parts[3],'base64url');bytes[0]^=1;parts[3]=bytes.toString('base64url');
reject(()=>consumer.open({{...event,ciphertext:parts.join('.')}},trusted));
let audits=0;
const modern={{namespace:config.namespace,tenantId:config.tenantId,activeKey:{{id:'active',key:Buffer.alloc(32,8)}},previousKeys:[{{id:'primary',key:config.key}}],actor:'interop',audit:async record=>{{if(JSON.stringify(record).includes('private marker'))throw Error('audit leak');audits++;return true;}}}};
const validate=value=>typeof value?.input?.message === 'string';
const audited=createAuditedWorkConsumer(modern,'jobs',validate);
const expected={{tenantId:config.tenantId,resourceId:'33333333-3333-4333-8333-333333333333',operationId:trusted.operationId,revision:1}};
if((await audited.consume(event,expected)).input.message !== 'private marker' || audits !== 1)throw Error('audited rotation failed');
const rejects=async fn=>{{let failed=false;try{{await fn();}}catch{{failed=true;}}if(!failed)throw Error('audited tamper accepted');}};
await rejects(()=>audited.consume(event,{{...expected,revision:2}}));
await rejects(()=>createAuditedWorkConsumer(modern,'other',validate).consume(event,expected));
await rejects(()=>createAuditedWorkConsumer(modern,'jobs',()=>false).consume(event,expected));
await rejects(()=>createAuditedWorkConsumer({{...modern,audit:async()=>false}},'jobs',validate).consume(event,expected));
await rejects(()=>createAuditedWorkConsumer({{...modern,auditTimeoutMs:5,audit:async()=>new Promise(()=>{{}})}},'jobs',validate).consume(event,expected));
audited.close();await rejects(()=>audited.consume(event,expected));
consumer.close();reject(()=>consumer.open(event,trusted));
",
        format!(
            "file://{}/../../../../framework-rs/crates/core/src/build/runtime/encrypted-worker.mjs",
            env!("CARGO_MANIFEST_DIR")
        ),
        serde_json::to_string(&sealed).unwrap(),
        serde_json::to_string(&wrong_purpose).unwrap()
    );
    let out = Command::new("node")
        .args(["--input-type=module", "-e", &source])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
