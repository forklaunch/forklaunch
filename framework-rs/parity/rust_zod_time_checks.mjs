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

const edges=['','00:00','23:59','24:00','00:60','1:01','01:1','00:00:00','23:59:59','00:00:60','00:00:00.1','00:00:00.123','00:00:00,123','00:00:00.1:01.2','00:00:00:01','00:00:00Z','00:00:00+00:00','٠٠:٠٠','００:００',null,1];
for(const value of ['00:00','00:00:00','00:00:00.123'])for(const suffix of ['\n','\r','\r\n','\u2028','\u2029','\0',' '])edges.push(value+suffix);
for(const precision of [null,0,1,2,3,9,1000]){
 const inputs=[...edges];
 for(let hour=0;hour<=24;hour++)for(let minute=0;minute<=60;minute++)for(const second of ['',':00',':01',':59',':60'])inputs.push(`${String(hour).padStart(2,'0')}:${String(minute).padStart(2,'0')}${second}`);
 for(const n of [0,1,2,3,4,8,9,10,999,1000,1001])for(const tail of ['',':00.'+'1'.repeat(n)])inputs.push('23:59:59.'+'1'.repeat(n)+tail);
 inputs.push('00:00:00.'+'1'.repeat(1000000));
 check(z.string().time({precision}),inputs);
}
for(const schema of [z.string().time().time({precision:3}),z.string().time({precision:1}).time({precision:2}),z.string().time().min(8).max(20),z.string().time().startsWith('00'),z.string().time().date()])check(schema,edges);
check(z.object({value:z.string().time().optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(z.string().time()),edges.map(value=>[value]));
check(z.union([z.string().time(),z.literal('alternate')]),[...edges,'alternate']);
for(const precision of [-1,0.5,1001,Infinity,NaN])assert.throws(()=>validator.openapi2020(z.string().time({precision}),{zodChecks:true}),/precision/);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string().time().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().time(),{zodChecks:true}),/transformations/);
for(const rules of [[-1],[1001],[0.5],['1'],[{}],Array(10001).fill(null)]){
 const schema={type:'string','x-forklaunch-zod-checks':{version:1,timePrecisions:rules}};
 assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(schema),/time precisions/);
 if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(schema)+'\n"00:00"\n'}));
}
const combined={type:'string','x-forklaunch-zod-checks':{version:1,timePrecisions:Array(5001).fill(null),formats:Array(5000).fill('date')}};
assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(combined),/10000 rules/);
if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(combined)+'\n"00:00"\n'}));
console.log(`PASS ${total} actual Zod/AJV time comparisons and ${nativeCount} native observations; bounded precision, optional/repeated seconds, all hour/minute boundaries and invalid-profile admission`);
