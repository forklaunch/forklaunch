import {actualValidator} from './rust_actual_validator_fixture.mjs';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default,addFormats=require('ajv-formats');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const {registerZodChecks}=load(path.join(root,'framework/universal-sdk/src/core/zodChecks.ts'));
const native=process.env.FORKLAUNCH_ZOD_CHECKS_NATIVE==='1';
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'));
function run(command,args,options={}){
 const out=spawnSync(command,args,{cwd:root,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'1'},encoding:'utf8',maxBuffer:64*1024*1024,...options});
 assert.equal(out.status,0,out.stdout+'\n'+out.stderr);return out.stdout;
}
if(native)run('cargo',['build','--locked','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','zod-checks','--example','uri_schema_parity']);
let total=0,nativeCount=0;
function check(schema,inputs){
 for(const pureInput of [false,true]){
  const baseline=validator.openapi2020(schema,{pureInput});
  assert.deepEqual(validator.openapi2020(schema,{pureInput,zodChecks:false}),baseline);
  const emitted=validator.openapi2020(schema,{pureInput,zodChecks:true});
  const engine=registerZodChecks(new Ajv({strict:false,ownProperties:true}));addFormats(engine);
  const validate=engine.compile(emitted),expected=inputs.map(v=>validator.parse(schema,v).ok);
  inputs.forEach((v,i)=>{assert.equal(!!validate(v),expected[i],JSON.stringify({emitted,v}));total++;});
  if(native){const actual=run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:[emitted,...inputs].map(v=>JSON.stringify(v)).join('\n')+'\n'}).trim().split('\n').map(JSON.parse);assert.deepEqual(actual,expected);nativeCount+=actual.length;}
 }
}

const lone=String.fromCharCode(0xd800), replacement=String.fromCodePoint(0xfffd);
const encode=(header,encoding='utf8')=>Buffer.from(header,encoding).toString('base64url')+'.x.';
const headers=new Set(['{}','[]','null','true','1','"HS256"','{"alg":"HS256"}','{"alg":"RS256","typ":"JWT"}','{"alg":"HS256","alg":""}','{"alg":false,"alg":"HS256"}','{"typ":"bad","typ":"JWT","alg":"HS256"}','{"alg":"\\ud800"}','{"alg":"\\ufffd"}','{"alg":"\\ud83d\\ude00"}','{"al\\u0067":"HS256"}','{"__proto__":{"alg":"HS256"}}','{"alg":1e999}','{"alg":-1e999}','{"alg":1e-999}','{"alg":-0}','{"alg":5e-324}','{"alg":2e-324}']);
for(const alg of ['HS256','RS256','',null,false,true,0,1,-1,[],{},['HS256'],{a:1},'é','😀','\ud800','\ufffd'])for(const typ of [undefined,'JWT','jwt','',null,false,true,0,[],{},'\ud800'])headers.add(JSON.stringify({alg,...(typ===undefined?{}:{typ})}));
for(const sign of ['','-','+'])for(const coefficient of ['0','1','2','5','9','0.1','1.0','1.5','01','1.','1e','NaN','Infinity'])for(const exponent of [-10000,-325,-324,-323,-308,0,308,309,10000])headers.add('{"alg":'+sign+coefficient+'e'+exponent+'}');
for(const extra of ['','x','\n','\r','\t','\0',' ','\ufeff'])for(const sample of ['{"alg":"HS256"}','{"alg":true}','{"alg":[]}']){headers.add(sample+extra);headers.add(extra+sample);}
for(const junk of ['[1,]','{"x":1,}','[1 2]','{"x" 1}','[','{','[truefalse]','{"x":"\\x20"}','{"x":"\\u123"}'])headers.add('{"alg":"HS256","ignored":'+junk+'}');
for(const length of [1,10,1000,20000]){headers.add('{"alg":'+ '['.repeat(length)+'0'+']'.repeat(length)+'}');headers.add('{"alg":"HS256","ignored":'+ '{"a":'.repeat(length)+'0'+'}'.repeat(length)+'}');}
for(let b=0;b<256;b++)headers.add('{"alg":"'+String.fromCharCode(b)+'"}');
headers.add(JSON.stringify({alg:lone}));headers.add(JSON.stringify({alg:replacement}));headers.add(JSON.stringify({alg:"HS256",[lone]:[lone]}));
headers.add(JSON.stringify({alg:'😀'}).replace('😀',String.fromCharCode(92)+'ud83d'+String.fromCharCode(92)+'ude00'));
headers.add('{"'+String.fromCharCode(92)+'u0061lg":"HS256"}');
const inputs=new Set();for(const header of headers){inputs.add(encode(header));inputs.add(encode(header,'latin1'));}
const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
for(const header of ['{"alg":"HS256"}','{"alg":"HS256","typ":"JWT"}']){
 const encoded=encode(header).split('.')[0];
 for(const last of alphabet)inputs.add(encoded.slice(0,-1)+last+'.x.');
 for(const payload of ['', 'x','!','e30','e30=','a.b','_'])for(const signature of ['', 'x','!','a=b','a.b','_'])inputs.add(encoded+'.'+payload+'.'+signature);
 let seed=19;
 for(let n=0;n<2000;n++){const bytes=Buffer.from(header);seed=(Math.imul(seed,1664525)+1013904223)>>>0;bytes[seed%bytes.length]=(seed>>>16)&255;inputs.add(bytes.toString('base64url')+'.x.');}
}
inputs.add('a.x.');inputs.add('____.x.');inputs.add('é.x.');
const values=[...inputs,null,1];
for(const alg of [undefined,null,'','HS256','RS256','é','Ã©','😀','\ufffd'])check(z.string().jwt({alg}),values);
const edges=values.slice(0,150);
for(const schema of [z.string().jwt().jwt({alg:'HS256'}),z.string().jwt({alg:'HS256'}).jwt({alg:'RS256'}),z.string().jwt().min(100),z.string().jwt().startsWith('ey'),z.string().jwt().date()])check(schema,edges);
check(z.object({value:z.string().jwt().optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(z.string().jwt()),edges.map(value=>[value]));
check(z.union([z.string().jwt(),z.literal('alternate')]),[...edges,'alternate']);
for(const alg of [1,{},[]])assert.throws(()=>validator.openapi2020(z.string().jwt({alg}),{zodChecks:true}),/algorithm/);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string().jwt().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().jwt(),{zodChecks:true}),/transformations/);
for(const rules of [[1],[{}],[[]],Array(10001).fill(null)]){
 const schema={type:'string','x-forklaunch-zod-checks':{version:1,jwtAlgorithms:rules}};
 assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(schema),/JWT algorithms/);
 if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(schema)+'\n"x.x."\n'}));
}
const combined={type:'string','x-forklaunch-zod-checks':{version:1,jwtAlgorithms:Array(5001).fill(null),formats:Array(5000).fill('date')}};
assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(combined),/10000 rules/);
if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(combined)+'\n"x.x."\n'}));
console.log(`PASS ${total} actual Zod/AJV JWT comparisons and ${nativeCount} native observations; header policy, Latin1 decoding, lone surrogate escapes, deep JSON, number truthiness and overrides`);
