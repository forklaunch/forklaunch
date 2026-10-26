import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
class UniqueConstraintViolationException extends Error{}
class NotFoundError extends Error{}
const key=event=>JSON.stringify([event.provider,event.providerEventId]);
class EntityManager {
 rows=new Map();pending=[];
 async transactional(callback){this.pending=[];try{const result=await callback(this);await this.flush();return result;}finally{this.pending=[];}}
 create(_entity,params){return{...params};}
 persist(event){this.pending.push(event);}
 async flush(){for(const event of this.pending){const identity=key(event);if(this.rows.has(identity)&&this.rows.get(identity)!==event)throw new UniqueConstraintViolationException();this.rows.set(identity,event);}this.pending=[];}
 async findOneOrFail(_entity,query){const event=this.rows.get(key(query));if(!event)throw new NotFoundError();return event;}
 assign(event,fields){Object.assign(event,fields);}
}
const source=await readFile(path.join(root,'blueprint/ecommerce-stripe/domain/services/webhookEvent.service.ts'),'utf8'),exports={};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(spec){
 if(spec==='@mikro-orm/core')return{EntityManager,UniqueConstraintViolationException};
 if(spec==='../../persistence/entities/webhookEvent.entity')return{WebhookEvent:'WebhookEvent'};
 throw Error('unexpected runtime import '+spec);
}});
const warnings=[],service=new exports.WebhookEventService(new EntityManager(),{info(){},warn(...args){warnings.push(args);}});
const event={provider:'stripe',providerEventId:'evt-1',eventType:'payment_intent.succeeded'};
const actions=[event,event,{operation:'mark',...event},event,{operation:'mark',...event},{...event,provider:'paypal'},{...event,provider:'paypal'}, {...event,providerEventId:'evt-2'}, {...event,providerEventId:'evt-2'}, {operation:'mark',provider:'stripe',providerEventId:'missing'}];
const expected=[];
for(const action of actions){try{const result=action.operation==='mark'?await service.markProcessed(action):await service.beginProcessing(action);expected.push({ok:result??null});}catch(error){assert(error instanceof NotFoundError);expected.push({error:'ecommerce_not_found'});}}
assert.equal(warnings.length,3);
const nativeActions=[...actions,{...event,eventType:'different-type'}, {...event,providerEventId:''}];
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','webhook_parity'],{cwd:root,input:JSON.stringify(nativeActions),encoding:'utf8',timeout:120000}));
assert.deepEqual(actual.slice(0,actions.length),expected);
assert.deepEqual(actual.slice(actions.length),[{error:'ecommerce_event_conflict'},{error:'ecommerce_validation'}]);
console.log(`PASS ${actions.length} actual TypeScript/Rust webhook ledger operations: new, unfinished retry, processed replay, provider-scoped identity, idempotent marks and native identity validation`);
