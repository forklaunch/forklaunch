// Actual extracted TypeScript method bodies; owned collaborators, no ORM bootstrap claim.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import vm from 'node:vm';
const require=createRequire(import.meta.url);
const ts=require(process.env.TSC_BIN?path.resolve(fs.realpathSync(process.env.TSC_BIN),'../../lib/typescript.js'):'typescript');
function method(file,names=['expireCheckoutSession']) {
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const selected=[];
 function visit(n){if(ts.isMethodDeclaration(n)&&names.includes(n.name?.getText(source)))selected.push(n);ts.forEachChild(n,visit);}
 visit(source);assert.equal(selected.length,names.length,file);
 const printed=selected.map(n=>ts.createPrinter().printNode(ts.EmitHint.Unspecified,n,source)).join('\n');
 const js=ts.transpileModule(`export class Probe { ${printed} }`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 const box={exports:{}};vm.runInNewContext(js,box);return box.exports.Probe;
}
const Base=method('blueprint/implementations/billing/base/services/checkoutSession.service.ts');
const Stripe=method('blueprint/implementations/billing/stripe/services/checkoutSession.service.ts',['expireCheckoutSession','stripeCheckoutSessionId']);
let cases=0;
// The stored Stripe id (cs_...) is resolved from the cached record before the
// provider call; the public id is never sent to Stripe. A record whose own id
// is a Stripe id and that stores no provider object still resolves to it.
for(const [id,stored,target] of [['00000000-0000-4000-8000-000000000001','cs_test_stored','cs_test_stored'],['cs_test_owned',undefined,'cs_test_owned'],['00000000-0000-4000-8000-000000000001',undefined,null],['00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000001',null]])for(const failure of ['none','provider','missing-cache','delete']){
 const calls=[]; const base=new Base(); base.createCacheKey=x=>`checkout:${x}`;
 base.getCheckoutSession=async({id:key})=>{calls.push(['lookup',key]);if(failure==='missing-cache')throw Error('Session not found');return stored===undefined?{id:key}:{id:key,stripeFields:{id:stored}};};
 base.cache={async readRecord(key){calls.push(['read',key]);return{value:{id}};},async deleteRecord(key){calls.push(['delete',key]);if(failure==='delete')throw Error('delete');}};
 const stripe=new Stripe();stripe.baseCheckoutSessionService=base;stripe.stripeClient={checkout:{sessions:{async expire(provider){calls.push(['provider',provider]);if(failure==='provider')throw Error('provider');return{id:provider,object:'checkout.session',status:'expired'};}}}};
 let error;try{await stripe.expireCheckoutSession({id});}catch(e){error=e.message;}
 assert.deepEqual(calls[0],['lookup',id]);
 if(failure==='missing-cache'){assert.equal(error,'Session not found');assert.equal(calls.length,1);}
 else if(target===null){assert.equal(error,`Checkout session ${id} has no stored Stripe checkout session id`);assert.equal(calls.length,1);}
 else{
  assert.deepEqual(calls[1],['provider',target]);
  if(failure==='provider'){assert.equal(error,'provider');assert.equal(calls.length,2);}else{
   assert.deepEqual(calls[2],['read',`checkout:${id}`]);
   assert.deepEqual(calls[3],['delete',`checkout:${id}`]);assert.equal(calls.length,4);assert.equal(error,failure==='delete'?'delete':undefined);
  }
 }
 cases++;
}
console.log(`PASS ${cases} actual TS Checkout expiration cases: stored Stripe id resolved before the provider call, public id never sent to Stripe, missing record or stored id fails before any effect, provider expiration precedes cache read/delete, local failure after remote effect, no ORM writes`);
