import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const sdkPath=process.env.STRIPE_SDK_PATH??require.resolve('stripe',{paths:[path.dirname(await realpath(process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc')))]});
const Stripe=require(sdkPath),stripe=new Stripe('sk_fixture');
const repository=JSON.parse(await readFile(path.join(root,'blueprint/ecommerce-stripe/package.json'),'utf8'));
assert.equal(repository.dependencies.stripe,`^${Stripe.PACKAGE_VERSION}`);
const now=1700000000,secret='whsec_fixture',body='{\n  "id":"evt-fixture", "message":"💬", "data":{}\n}';
const sign=(timestamp=now,payload=body,key=secret)=>stripe.webhooks.generateTestHeaderString({timestamp,payload,secret:key});
const valid=sign(),signature=valid.split('v1=')[1];
const cases=[
 {body,header:valid},{body:body+' ',header:valid},{body:'changed',header:valid},{body,header:sign(now,body,'different')},
 {body,header:sign(now-300)},{body,header:sign(now-301)},{body,header:sign(now+600)},
 {body,header:''},{body,header:`t=${now}`},{body,header:`v1=${signature}`},{body,header:`t=${now},v0=${signature}`},
 {body,header:`t=${now},v1=bad,v1=${signature}`},{body,header:`t=${now},v1=${signature},v1=bad`},
 {body,header:`t=${now}, v1=${signature}`},{body,header:`t=${now},v1=${signature.toUpperCase()}`},
 {body,header:`t=${now},v1=0${signature}`},{body,header:`t=${now},v1=${signature}=ignored`},
 {body,header:`t=${now-500},t=${now},v1=${signature}`},{body:'',header:sign(now,'')},
 {body:JSON.stringify(JSON.parse(body)),header:valid},
].map(testcase=>({secret,now,...testcase}));
const expected=cases.map(testcase=>{try{return stripe.webhooks.signature.verifyHeader(testcase.body,testcase.header,testcase.secret,300,undefined,now*1000);}catch{return false;}});
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-stripe','--example','webhook_signature_parity'],{cwd:root,input:JSON.stringify(cases),encoding:'utf8',timeout:120000}));
assert.deepEqual(actual,expected);
console.log(`PASS ${cases.length} official Stripe ${Stripe.PACKAGE_VERSION}/Rust raw-body signature cases: timestamp boundary, SDK future-time semantics, rotated signatures, exact bytes and malformed headers`);
