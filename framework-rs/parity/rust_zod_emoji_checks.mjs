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

assert.equal(process.versions.node,'24.2.0','emoji acceptance oracle pins Node');
assert.equal(process.versions.unicode,'16.0','emoji acceptance oracle pins Unicode');
run('python3',['scripts/rust_zod_emoji_table.py']);
const schema=z.string().emoji();
let values=[];
for(let cp=0;cp<=0x10ffff;cp++){
 if(cp>=0xd800&&cp<=0xdfff)continue;
 values.push(String.fromCodePoint(cp));
 if(values.length===65536){check(schema,values);values=[];}
}
if(values.length)check(schema,values);
const edges=['','😀','👩‍💻','🏳️‍🌈','#0123456789*','\u200d','\ufe0f','\u20e3','a','😀a','a😀','😀\n','😀\r','😀\u2028','😀\0','😀 '.repeat(10),'😀'.repeat(10000),null,1];
check(schema,edges);
for(const source of [z.string().emoji().emoji(),z.string().emoji().min(2).max(4),z.string().emoji().startsWith('😀'),z.string().emoji().date()])check(source,edges);
check(z.object({value:schema.optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(schema),edges.map(value=>[value]));
check(z.union([schema,z.literal('alternate')]),[...edges,'alternate']);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(schema.openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().emoji(),{zodChecks:true}),/transformations/);
console.log(`PASS ${total} actual Zod/AJV emoji comparisons and ${nativeCount} native observations; every Unicode scalar, components, sequences, nesting and overrides`);
