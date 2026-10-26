import assert from 'node:assert/strict';
import http from 'node:http';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
import {actualUniversalSdk} from './fixtures/rust_better_auth_frontend_client.mjs';
assert.equal(process.versions.node,'24.2.0','Pinned regex semantic target');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const Ajv2020=require('ajv/dist/2020').default;
const {validator,statics}=actualValidator(root);
const {UniversalSdk,universalSdk}=await actualUniversalSdk();
const engine=new Ajv2020({strict:false,allErrors:true,ownProperties:true});
new UniversalSdk('http://unused',engine,{},undefined,undefined,'host-native');
const opts={pureInput:true,zodRegex:true};
const values=['','a','A','aa','ab','ba','b',' a ','\n','a\n','K','ſ','σ','ς','😀','\ud800','é','水',null,1];
const patterns=['.','^.$','^a$','a+','[a-z]','\\w','k','s','Σ','(?<=a)b','(?=a)a','(a)\\1','(?<x>a)\\k<x>','[\\p{ASCII}&&\\p{Letter}]','[\\p{ASCII}--[A-Z]]','[\\q{ab|a}]','\\p{Letter}','[^]'];
let comparisons=0;
for(const pattern of patterns)for(const flags of ['','i','u','v','iu','iv','g','y']) {
 let regex;try{regex=new RegExp(pattern,flags);}catch{continue;}
 const source=statics.string.regex(regex).regex(/^[\s\S]*$/);
 const schema=validator.openapi2020(source,opts);
 assert.equal(schema.pattern,undefined);
 assert.equal(schema['x-forklaunch-zod-regex'].rules.length,2);
 const validate=engine.compile(schema);
 for(const value of values) {
  assert.equal(validate(value),validator.parse(source,value).ok,JSON.stringify({pattern,flags,value}));
  comparisons++;
 }
 assert.equal(validator.openapi2020(source).pattern,'^[\\s\\S]*$');
}
const source=statics.string.regex(/^a/g).regex(/z$/y);
for(const pattern of [undefined,null,'^override$']) {
 const annotated=source.openapi({pattern});
 const schema=validator.openapi2020(annotated,{zodRegex:true});
 assert.equal(schema['x-forklaunch-zod-regex'],undefined);
 assert.equal(schema.pattern,pattern);
}
assert.throws(()=>validator.openapi2020(statics.string.trim().regex(/a/),{zodRegex:true}),/transforms/);
assert.doesNotThrow(()=>validator.openapi2020(statics.string.trim().regex(/a/).openapi({pattern:'a'}),{zodRegex:true}));
assert.throws(()=>validator.openapi2020(source.openapi({'x-forklaunch-zod-regex':{}}),opts),/reserved/i);
assert.throws(()=>validator.openapi2020(statics.string.regex(new RegExp('a'.repeat(4097))),opts),/limit/);
const schema=validator.openapi2020(statics.string.regex(/^a/).regex(/z$/),opts);
const disabled=new Ajv2020({strict:false});
new UniversalSdk('http://unused',disabled,{},undefined);
assert.throws(()=>disabled.compile(schema),/host-native/);
assert.doesNotThrow(()=>disabled.compile({const:{'x-forklaunch-zod-regex':{bad:'literal'}}}));
for(const malformed of [{version:2,rules:[]},{version:1,rules:[{sourceUnits:[65536],flags:''}]},{version:1,rules:[{sourceUnits:[],flags:'uv'}]},{version:1,rules:[{sourceUnits:[],flags:'gg'}]}])
 assert.throws(()=>engine.compile({'x-forklaunch-zod-regex':malformed}),/Invalid/);
const before={...engine.opts};
new UniversalSdk('http://unused',engine,{},undefined,undefined,'host-native');
assert.deepEqual(engine.opts,before);
const conflicting=new Ajv2020({strict:false}).addKeyword({keyword:'x-forklaunch-zod-regex',validate:()=>true});
assert.throws(()=>new UniversalSdk('http://unused',conflicting,{},undefined,undefined,'host-native'),/Conflicting/);
let payload='abz';
const server=http.createServer((_,res)=>{res.setHeader('content-type','application/json');res.end(JSON.stringify(payload));});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try {
 const host=`http://127.0.0.1:${server.address().port}`;
 const doc={openapi:'3.1.0',info:{title:'Regex',version:'1'},paths:{'/value':{get:{operationId:'value',responses:{200:{content:{'application/json':{schema}}}}}}}};
 const snapshot=JSON.stringify(doc);
 const sdk=await universalSdk({host,registryOptions:{raw:doc},zodRegexExecution:'host-native'});
 assert.equal((await sdk.value()).response,'abz');
 payload='a!';await assert.rejects(()=>sdk.value());
 const off=await universalSdk({host,registryOptions:{raw:doc}});
 await assert.rejects(()=>off.value(),/host-native/);
 assert.equal(JSON.stringify(doc),snapshot);
} finally {await new Promise(resolve=>server.close(resolve));}
console.log(`PASS ${comparisons} actual Zod/explicit host-native AJV regex comparisons, override/budget/policy controls and 3 live HTTP calls`);
