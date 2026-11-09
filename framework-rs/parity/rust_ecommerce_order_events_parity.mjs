import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const OrderStatus=Object.fromEntries(['pending','paid','fulfilled','shipped','delivered','cancelled'].map(value=>[value.toUpperCase(),value]));
const inventory={a:10,b:2};let stock={...inventory};let processEvents;
const service={em:{async transactional(fn){const before={...stock};try{return await fn(this);}catch(error){stock=before;throw error;}}},async adjustStock({variantId,delta}){if(!(variantId in stock))throw Error('missing');if(delta<0&&stock[variantId]<-delta)throw Error('insufficient');stock[variantId]+=delta;}};
const tokens=new Proxy({}, {get:(_,key)=>key});
const ci={resolve(token){if(token==='OtelCollector')return {info(){},error(){}};if(token==='OrderEventConsumer')return (process,_failure)=>{processEvents=process;return {start(){}};};throw Error('unexpected token '+token);},scopedResolver(token){assert.equal(token,'InventoryService');return ()=>service;}};
const source=await readFile(path.join(root,'blueprint/ecommerce-stripe/worker.ts'),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
vm.runInNewContext(compiled,{exports:{},require(spec){if(spec==='./bootstrapper')return {ci,tokens};if(spec==='@forklaunch/interfaces-ecommerce/types')return {OrderStatus};throw Error('unexpected import '+spec);}});
assert.equal(typeof processEvents,'function');
const steps=[];let sequence=0;
const event=(from,to,items,id=`event-${sequence++}`)=>({event:{id,orderId:'order',fromStatus:from,toStatus:to,items:items.map(([variantId,quantity])=>({variantId,quantity,unitPriceCents:100})),processed:false,retryCount:0}});
steps.push(event('pending','paid',[['a',2],['b',3]],'retry'));
steps.push({adjust:{variantId:'b',delta:8}});
steps.push(event('pending','paid',[['a',2],['b',3]],'retry'));
steps.push(event('paid','cancelled',[['a',2],['b',3]]));
steps.push(event('pending','cancelled',[['a',3]]));
steps.push(event('pending','paid',[['a',1],['a',2]]));
steps.push(event('paid','fulfilled',[['a',3]]));
steps.push(event('fulfilled','cancelled',[['a',3]]));
steps.push(event('pending','paid',[['a',1]]));
steps.push(event('paid','fulfilled',[['a',1]]));
steps.push(event('fulfilled','shipped',[['a',1]]));
steps.push(event('shipped','delivered',[['a',1]]));
steps.push(event('pending','paid',[['a',1],['missing',1]]));
steps.push(event('pending','paid',[['a',1000]]));
const expected=[];
for(const step of steps) {
 if(step.adjust){await service.adjustStock(step.adjust);continue;}
 const payload=structuredClone(step.event);const failures=await processEvents([payload]);
 expected.push({failed:failures.length!==0,processed:payload.processed,stock:{...stock}});
}
// Per-batch failure reporting must preserve successful items and continue.
const batch=[event('pending','paid',[['a',1000]]).event,event('pending','cancelled',[['a',1]]).event];
const failed=await processEvents(batch);assert.equal(failed.length,1);assert.equal(failed[0].value.id,batch[0].id);assert.equal(batch[1].processed,true);
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-base','--example','order_events_parity'],{cwd:root,input:JSON.stringify({inventory,steps}),encoding:'utf8',timeout:120000}));
assert.equal(actual.length,expected.length);for(let i=0;i<expected.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(steps.filter(step=>step.event)[i]));
console.log(`PASS ${expected.length} actual TypeScript/Rust order inventory events: all legal transitions, all-item rollback, retry, missing inventory, duplicate variants and pending/paid/fulfilled cancellation`);
