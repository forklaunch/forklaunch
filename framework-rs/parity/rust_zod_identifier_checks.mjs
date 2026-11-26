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
const methods=['cuid','cuid2','ulid','nanoid','base64','base64url'];
const native=process.env.FORKLAUNCH_ZOD_CHECKS_NATIVE==='1';
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'));
function run(command,args,options={}){
 const out=spawnSync(command,args,{cwd:root,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'1'},encoding:'utf8',maxBuffer:64*1024*1024,...options});
 assert.equal(out.status,0,out.stdout+'\n'+out.stderr);return out.stdout;
}
if(native)run('cargo',['build','--locked','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','zod-checks','--example','uri_schema_parity']);
const seeds=['','a','A','0','a'.repeat(8),'c'+'a'.repeat(8),'C'+'水'.repeat(8),'c'+'😀'.repeat(4),'c'+'😀'.repeat(3),'c'+'\0'.repeat(8),'a'.repeat(21),'0'.repeat(26),'z'.repeat(26),'I'.repeat(26),'a@b.co','AA==','AB==','AAA=','AAB=','AAAA','AA','AAA','_w','/w==','====','AA=','AAA==','AA==='];
const values=[...seeds,null,0,true];
for(const seed of seeds)for(const tail of ['\n','\r','\r\n','\u2028','\u2029','\0',' ','\u0085','\ufeff','-'])values.push(seed+tail);
for(let point=0;point<256;point++){
 const c=String.fromCodePoint(point);
 values.push(c.repeat(21),c.repeat(26),'c'+c.repeat(8),c.repeat(4),c+'A==');
}
for(const point of [0x1680,0x180e,0x2000,0x200a,0x200b,0x2028,0x2029,0x202f,0x205f,0x212a,0x017f,0x3000,0xfeff,0x1f600]){
 const c=String.fromCodePoint(point);values.push('c'+c.repeat(8),'c'+c.repeat(4),c.repeat(21),c.repeat(26));
}
for(let n=0;n<40;n++)for(let pad=0;pad<5;pad++)for(const c of ['A','+','/','_','-'])values.push(c.repeat(n)+'='.repeat(pad));
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
for(const method of methods){
 const base=z.string()[method]();check(base,values);
 check(base,['A'.repeat(65536),'A'.repeat(1048576),'A'.repeat(65536)+'!','c'+'😀'.repeat(32768)]);
 for(const next of methods)check(base[next](),seeds);
 check(base.min(2).max(26),values);
 check(z.object({value:base.optional()}),[{},...seeds.map(value=>({value}))]);
 check(z.array(base),seeds.map(value=>[value]));
 check(z.union([base,z.literal('alternate')]),[...seeds,'alternate']);
 for(const format of ['uuid',undefined,null]){
  const out=validator.openapi2020(base.openapi({format}),{zodChecks:true});
  assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);
 }
 const pattern=validator.openapi2020(base.openapi({pattern:'^a'}),{zodChecks:true});
 assert.equal(pattern.pattern,'^a');assert.deepEqual(pattern['x-forklaunch-zod-checks'].formats,[method]);
 assert.throws(()=>validator.openapi2020(z.string().trim()[method](),{zodChecks:true}),/transformations/);
}
// JavaScript-only strings outside Rust's representation are still exact in its SDK.
for(const method of methods){const schema=z.string()[method]();const out=validator.openapi2020(schema,{zodChecks:true});const valid=registerZodChecks(new Ajv({strict:false})).compile(out);for(const value of ['c'+'\ud800'.repeat(8),'c'+'\udfff'.repeat(8)])assert.equal(!!valid(value),validator.parse(schema,value).ok);}
console.log(`PASS ${total} actual Zod/AJV identifier/Base64 comparisons and ${nativeCount} native observations; bounds, Unicode, padding, conjunctions, nested schemas and explicit overrides`);
