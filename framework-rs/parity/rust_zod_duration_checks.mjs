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

const values=new Set(['','P','PT','+P','-P','P1Y','P1Y2M3W4DT5H6M7S','-P-1Y+2M-3W+4DT-5H+6M-7S','P1.5Y','P1,5Y','P1.5Y1M','PT1.5H','PT1.5H1M','P1DT','PTT1S','P1S','PT1D']);
const parts=['0','1','01','-1','+1','1.5','1,5','-0.5','+0,5','1.','.5','1e3','','--1','١','１'];
for(const prefix of ['P','+P','-P','PT','+PT','-PT'])for(const part of parts)for(const unit of ['Y','M','W','D','H','S','y',''])values.add(prefix+part+unit);
for(const a of ['Y','M','W','D','H','S'])for(const b of ['Y','M','W','D','H','S'])for(const n of ['1','-1','1.2','1,2'])for(const bridge of ['','T']){values.add('P'+n+a+bridge+'2'+b);values.add('PT'+n+a+bridge+'2'+b);}
for(let mask=1;mask<128;mask++){let s='P';for(let i=0;i<7;i++){if(i===4&&mask>>4)s+='T';if(mask&(1<<i))s+=String(i+1)+'YMWDHMS'[i];}values.add(s);}
let state=123456789;
const alphabet='PTHMSYWD0129+-,. abc\n';
for(let i=0;i<15000;i++){let s='';state=(Math.imul(state,1664525)+1013904223)>>>0;const n=state%24;for(let j=0;j<n;j++){state=(Math.imul(state,1664525)+1013904223)>>>0;s+=alphabet[state%alphabet.length];}values.add(s);}
for(const s of ['P1Y','P1.5Y','PT1S','P1DT1S'])for(const suffix of ['\n','\r','\r\n','\u2028','\u2029','\0',' ','T'])values.add(s+suffix);
values.add('P'+'9'.repeat(1000000)+'Y');values.add('P'+'9'.repeat(1000000)+'Y!');
const inputs=[...values,null,1];
check(z.string().duration(),inputs);
const edges=inputs.slice(0,100);
for(const schema of [z.string().duration().duration(),z.string().duration().min(3).max(20),z.string().duration().startsWith('P'),z.string().duration().date(),z.string().date().duration()])check(schema,edges);
check(z.object({value:z.string().duration().optional()}),[{},...edges.map(value=>({value}))]);
check(z.array(z.string().duration()),edges.map(value=>[value]));
check(z.union([z.string().duration(),z.literal('alternate')]),[...edges,'alternate']);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string().duration().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().duration(),{zodChecks:true}),/transformations/);
console.log(`PASS ${total} actual Zod/AJV duration comparisons and ${nativeCount} native observations; signed/ordered components, terminal fractions, nesting, overrides and million-digit inputs`);
