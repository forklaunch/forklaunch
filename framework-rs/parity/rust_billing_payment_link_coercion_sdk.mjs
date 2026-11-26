// Generated SDK from the live selected registry; no schema emulation.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,realpathSync,writeFileSync,mkdtempSync,rmSync,symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import vm from 'node:vm';
import {createHmac,randomUUID} from 'node:crypto';
const [file,base,id]=process.argv.slice(2),require=createRequire(import.meta.url);
const ts=require(require.resolve('typescript',{paths:[path.dirname(realpathSync(process.env.TSC_BIN))]}));
const source=readFileSync(file,'utf8');
// The generated SDK's schema runtime imports Ajv; resolve it from the pinned
// TypeScript fixture prefix (node_modules/typescript/bin/tsc -> prefix).
const sdkPrefix=path.dirname(path.dirname(path.dirname(path.dirname(realpathSync(process.env.TSC_BIN))))),sdkRequire=createRequire(path.join(sdkPrefix,'package.json'));
const exports={};vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:sdkRequire,fetch,URL,URLSearchParams,Headers,Request,Response,AbortController,console,TextEncoder,TextDecoder,structuredClone,crypto:globalThis.crypto});
const sdk=exports.createForkLaunchSdk({baseUrl:base});
function find(value,parts=[]){for(const [key,item]of Object.entries(value)){if(key==='updatePaymentLink'&&typeof item==='function')return {call:item.bind(value),parts:[...parts,key]};if(item&&typeof item==='object'){const result=find(item,[...parts,key]);if(result)return result;}}}
const found=find(sdk);assert(found);const {call,parts}=found;
const tmp=mkdtempSync(path.join(tmpdir(),'fl-link-coercion-types-'));
try {
 const access=parts.map(key=>'['+JSON.stringify(key)+']').join('');
 const checks=[null,'12',false,[],'Infinity',{},'bad'].map(amount=>`createForkLaunchSdk({baseUrl:'http://unused'})${access}({params:{id:'owned'},body:{id:'owned',amount:${JSON.stringify(amount)}}});`).join('\n');
 // Type-check against the same pinned Ajv declarations the runtime loads.
 symlinkSync(path.join(sdkPrefix,'node_modules'),path.join(tmp,'node_modules'),'dir');
 const target=path.join(tmp,'sdk.ts');writeFileSync(target,source+'\n'+checks);
 const checked=spawnSync(process.env.TSC_BIN,['--noEmit','--strict','--target','ES2022','--module','ESNext','--moduleResolution','bundler','--lib','ES2022,DOM,DOM.Iterable',target],{encoding:'utf8'});assert.equal(checked.status,0,checked.stdout+checked.stderr);
}finally{rmSync(tmp,{recursive:true,force:true});}
for(const amount of [null,'12',false,[],'Infinity',{},'bad']) {
 const body={id,amount},raw=JSON.stringify(body),stamp=new Date().toISOString(),nonce=randomUUID(),route=`/payment-link/${id}`;
 const signature=createHmac('sha256','billing-owner-fixture').update(`PUT\n${route.replace(/^\/payment-link/, '')}\n${raw}\n${stamp}\n${nonce}`).digest('base64');
 const result=await call({params:{id},body,headers:{authorization:`HMAC keyId=default ts=${stamp} nonce=${nonce} signature=${signature}`}});
 assert.equal(result.status,amount==='bad'||typeof amount==='object'&&amount!==null&&!Array.isArray(amount)?400:200,JSON.stringify(result));
 if(result.status===200)assert.equal(result.response.amount,34);
}
for(const kind of ['missing','invalid','full-path','old']) {
 const body={id,amount:'12'},raw=JSON.stringify(body),stamp=kind==='old'?'1900-01-01T00:00:00.000Z':new Date().toISOString(),nonce=randomUUID();
 const route=kind==='full-path'?`/payment-link/${id}`:`/${id}`;
 const signature=kind==='invalid'?'bad':createHmac('sha256','billing-owner-fixture').update(`PUT\n${route}\n${raw}\n${stamp}\n${nonce}`).digest('base64');
 const headers=kind==='missing'?{}:{authorization:`HMAC keyId=default ts=${stamp} nonce=${nonce} signature=${signature}`};
 const result=await call({params:{id},body,headers});// The full wire path verifies (0a52af5a0); the router-relative form is still
 // accepted by default (479a42ddb).
 const accepted=kind==='old'||kind==='full-path';
 assert.equal(result.status,kind==='missing'?401:accepted?200:403);
 if(kind==='missing')assert.equal(result.response,'Authorization token required.');else if(!accepted)assert.equal(result.response,'Invalid Authorization signature.');else assert.equal(result.response.amount,34);
}
const aliasSdk=exports.createForkLaunchSdk({baseUrl:base,beforeRequest:meta=>({...meta,path:meta.path+'/'})});
const alias=find(aliasSdk).call,aliasBody={id,amount:'12'},aliasStamp=new Date().toISOString(),aliasNonce=randomUUID();
const aliasSignature=createHmac('sha256','billing-owner-fixture').update(`PUT\n/${id}/\n${JSON.stringify(aliasBody)}\n${aliasStamp}\n${aliasNonce}`).digest('base64');
const aliasResult=await alias({params:{id},body:aliasBody,headers:{authorization:`HMAC keyId=default ts=${aliasStamp} nonce=${aliasNonce} signature=${aliasSignature}`}});assert.equal(aliasResult.status,200);assert.equal(aliasResult.response.amount,34);
console.log('PASS twelve generated TypeScript SDK calls: coercion/authentication and selected trailing-slash transport alias');
