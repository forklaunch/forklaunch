// Actual extracted TypeScript method bodies; owned collaborators, no ORM bootstrap claim.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import vm from 'node:vm';
const require=createRequire(import.meta.url);
const ts=require(process.env.TSC_BIN?path.resolve(fs.realpathSync(process.env.TSC_BIN),'../../lib/typescript.js'):'typescript');
function method(file,names=['expirePaymentLink']) {
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const selected=[];
 function visit(n){if(ts.isMethodDeclaration(n)&&names.includes(n.name?.getText(source)))selected.push(n);ts.forEachChild(n,visit);}
 visit(source);assert.equal(selected.length,names.length,file);
 const printed=selected.map(n=>ts.createPrinter().printNode(ts.EmitHint.Unspecified,n,source)).join('\n');
 const js=ts.transpileModule(`export class Probe { ${printed} }`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 const box={exports:{}};vm.runInNewContext(js,box);return box.exports.Probe;
}
const Base=method('blueprint/implementations/billing/base/services/paymentLink.service.ts');
const Stripe=method('blueprint/implementations/billing/stripe/services/paymentLink.service.ts',['expirePaymentLink','stripePaymentLinkId']);
const id='00000000-0000-4000-8000-000000000001';
let cases=0;
// The public UUID never reaches Stripe: the stored Stripe id is resolved from
// the cached record first, and a record without one fails before any effect.
for(const backup of [false,true])for(const failure of ['missing-record','no-stored-id','foreign-stored-id']){
 const calls=[];const base=new Base();base.enableDatabaseBackup=backup;base.openTelemetryCollector={info(){}};base.createCacheKey=x=>`payment-link:${x}`;
 base.getPaymentLink=async({id:key})=>{calls.push(['read',key]);if(failure==='missing-record')throw Error('Payment link not found');return failure==='no-stored-id'?{id:key}:{id:key,stripeFields:{id:key}};};
 base.cache={async deleteRecord(key){calls.push(['delete',key]);}};
 const stripe=new Stripe();stripe.basePaymentLinkService=base;stripe.stripeClient={paymentLinks:{async update(target){calls.push(['provider',target]);return{id:target};}}};
 let error;try{await stripe.expirePaymentLink({id});}catch(e){error=e.message;}
 assert.deepEqual(calls,[['read',id]]);
 assert.equal(error,failure==='missing-record'?'Payment link not found':`Payment link ${id} has no stored Stripe payment link id`);
 cases++;
}
for(const backup of [false,true])for(const failure of ['none','provider','local']){
 const calls=[];const base=new Base();base.enableDatabaseBackup=backup;base.openTelemetryCollector={info(){}};base.createCacheKey=x=>`payment-link:${x}`;
 base.getPaymentLink=async({id:key})=>{calls.push(['read',key]);return{id:key,stripeFields:{id:'plink_actual',object:'payment_link'}};};
 base.cache={async deleteRecord(key){calls.push(['delete',key]);if(failure==='local')throw Error('local');}};
 base.em={async upsert(entity,record){calls.push(['upsert',JSON.parse(JSON.stringify(record))]);return record;},remove(record){calls.push(['remove',JSON.parse(JSON.stringify(record))]);return{async flush(){calls.push(['flush']);}};}};
 base.mappers={PaymentLinkMapper:{entity:class{}}};
 const stripe=new Stripe();stripe.basePaymentLinkService=base;stripe.stripeClient={paymentLinks:{async update(target,options){calls.push(['provider',target,JSON.parse(JSON.stringify(options))]);if(failure==='provider')throw Error('provider');return{id:'plink_actual',active:true};}}};
 let error;try{await stripe.expirePaymentLink({id});}catch(e){error=e.message;}
 assert.deepEqual(calls[0],['read',id]);
 // Stripe is called with the stored Stripe id, never the public UUID.
 assert.deepEqual(calls[1],['provider','plink_actual',{metadata:{status:'EXPIRED'}}]);
 if(failure==='provider'){assert.equal(error,'provider');assert.equal(calls.length,2);}else{
  assert.deepEqual(calls.at(-1),['delete',`payment-link:${id}`]);
  assert.equal(error,failure==='local'?'local':undefined);
  if(backup){assert.deepEqual(calls[2],['upsert',{id,status:'EXPIRED'}]);assert.equal(calls[3][0],'remove');assert.equal(calls[4][0],'flush');}
  else assert.equal(calls.length,3);
 }
 cases++;
}
console.log(`PASS ${cases} actual TS PaymentLink expiration method cases: stored Stripe id resolved before the metadata-only update, public UUID never sent to Stripe, missing record or stored id fails before any effect, cache delete/backup removal, pre/post effect failures`);
