// Run after node_channel with FORKLAUNCH_CHANNEL_ASSET_FIXTURE and dependency install.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {execFileSync} from 'node:child_process';
const directory=resolve(process.argv[2]);
writeFileSync(join(directory,'consumer.ts'),`
import {createNoteJobProducer,createInsightEventConsumer} from './events.js';
import type {NoteJob,InsightEvent,ProducerOptions,ConsumerOptions,ChannelContext} from './events.js';
export function example(producer:ProducerOptions, consumer:ConsumerOptions, context:ChannelContext, job:NoteJob) {
  createNoteJobProducer(producer).publish(job,context);
  // @ts-expect-error actual NoteJob requires a string body
  createNoteJobProducer(producer).publish({...job,payload:{...job.payload,body:42}},context);
  const result:Promise<InsightEvent> = createInsightEventConsumer(consumer).consume({},context);
  return result;
}
`);
execFileSync(process.execPath,[join(directory,'node_modules/typescript/bin/tsc'),'events.ts','consumer.ts','--target','ES2022','--module','NodeNext','--moduleResolution','NodeNext','--strict','--skipLibCheck'],{cwd:directory,stdio:'inherit'});
const {createNoteJobProducer,createNoteJobConsumer,createInsightEventProducer,createInsightEventConsumer}=await import(pathToFileURL(join(directory,'events.js')));
const context={tenantId:'11111111-1111-4111-8111-111111111111',operationId:'22222222-2222-4222-8222-222222222222',resourceId:'33333333-3333-4333-8333-333333333333',revision:1};
const payload={workspaceId:context.tenantId,operationId:context.operationId,noteId:context.resourceId,revision:1,body:'private generated note'};
const job={payload,trace:'00-0123456789abcdef0123456789abcdef-0123456789abcdef-01'};
const options={namespace:'notes',tenantId:context.tenantId,activeKey:{id:'primary',key:Buffer.alloc(32,7)}};
let envelope,audited=0;
const producer=createNoteJobProducer({...options,publish:async value=>{envelope=value;assert(!JSON.stringify(value).includes(job.payload.body));}});
await producer.publish(job,context);
assert.deepEqual(Object.keys(envelope).sort(),['workspaceId','noteId','operationId','revision','payload'].sort());
const consumer=createNoteJobConsumer({...options,actor:'worker',audit:async record=>{audited++;assert(!JSON.stringify(record).includes(job.payload.body));return true;}});
assert.deepEqual(await consumer.consume(envelope,context),job);assert.equal(audited,1);
await assert.rejects(producer.publish({...job,payload:{...job.payload,body:42}},context));
await assert.rejects(producer.publish({...job,extra:'forbidden'},context));
await assert.rejects(producer.publish({...job,payload:{...job.payload,noteId:'invalid UUID'}},context));
await assert.rejects(producer.publish(payload,context));
await assert.rejects(producer.publish({...job,trace:'00-00000000000000000000000000000000-0123456789abcdef-01'},context));
const denied=createNoteJobConsumer({...options,actor:'worker',audit:async()=>false});
await assert.rejects(denied.consume(envelope,context));
denied.close();
await assert.rejects(consumer.consume(envelope,{...context,tenantId:'44444444-4444-4444-8444-444444444444'}));
assert.equal(audited,1);
assert(!JSON.stringify(envelope).includes(job.trace));
const before=envelope;
for (const replacement of [{...job,payload:{...job.payload,body:42}},{...job,payload:{...job.payload,revision:2}}]) {
  const transformed={...job};Object.defineProperty(transformed,'toJSON',{value:()=>replacement});
  await assert.rejects(producer.publish(transformed,context));assert.equal(envelope,before);
}
assert.throws(()=>createNoteJobConsumer({...options,actor:'worker\nforged',audit:async()=>true}));

const result={workspaceId:context.tenantId,operationId:context.operationId,noteId:context.resourceId,revision:1,excerpt:'private result',wordCount:2};
const resultProducer=createInsightEventProducer({...options,publish:async value=>{envelope=value;}});
await resultProducer.publish(result,context);
assert.equal(envelope.channel,'note-insight-result-v1');
const resultConsumer=createInsightEventConsumer({...options,actor:'api',audit:async()=>true});
assert.deepEqual(await resultConsumer.consume(envelope,context),result);
await assert.rejects(resultProducer.publish({...result,wordCount:-1},context));
const manifest=JSON.parse(readFileSync(join(directory,'event-contract.json'),'utf8'));
assert.deepEqual(manifest.channels.map(value=>value.purpose),['note-insight-job-v2','note-insight-result-v1']);
producer.close();consumer.close();resultProducer.close();resultConsumer.close();
console.log('Generated Notes channel types, schemas, encrypted wire shapes and audited consumers passed');
