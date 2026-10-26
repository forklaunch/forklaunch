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
const dates=[];
for(let year=0;year<10000;year++)for(const day of [28,29,30])dates.push(`${String(year).padStart(4,'0')}-02-${day}`);
for(const year of [0,1,4,100,400,1900,2000,2024,2100,9999])for(let month=0;month<=13;month++)for(let day=0;day<=32;day++)dates.push(`${String(year).padStart(4,'0')}-${String(month).padStart(2,'0')}-${String(day).padStart(2,'0')}`);
const edges=['','0000-02-29','2024-02-29','1900-02-29','2000-02-29','2024-01-01','2024-1-01','2024-01-1','02024-01-01','-001-01-01','2024-01-01T00:00:00Z','٢٠٢٤-٠١-٠١','２０２４-０１-０１',null,1];
for(const value of ['0000-02-29','2024-02-29','2024-01-01'])for(const suffix of ['\n','\r','\r\n','\u2028','\u2029','\0',' ','Z','+00:00'])edges.push(value+suffix);
check(z.string().date(),[...dates,...edges]);
for(const schema of [z.string().date().date(),z.string().date().min(10).max(10),z.string().date().startsWith('2024'),z.string().date().email(),z.string().email().date()])check(schema,edges);
check(z.object({value:z.string().date().optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(z.string().date()),edges.map(value=>[value]));
check(z.union([z.string().date(),z.literal('alternate')]),[...edges,'alternate']);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string().date().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().date(),{zodChecks:true}),/transformations/);
console.log(`PASS ${total} actual Zod/AJV date comparisons and ${nativeCount} native observations; every four-digit leap-year boundary, month/day limits, Unicode, nesting and overrides`);
