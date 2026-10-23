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

const dates=['0000-02-29','1900-02-29','2000-02-29','2024-02-29','2024-02-30','2023-12-31','9999-12-31','2024-01-00','2024-13-01','2024-1-01','２０２４-01-01'];
const times=['00:00','23:59','24:00','00:60','00:00:00','23:59:59','00:00:60','00:00:00.1','00:00:00.123','00:00:00.1:01.2','00:00:00:01','00:00:00.'+'1'.repeat(1000),'00:00:00.'+'1'.repeat(1001)];
const zones=['','Z','z','+00:00','-00:00','+0000','-9999','+99:99','+24:60','+0:00','+00','+000','+00:000','+００:００'];
const edges=[];
for(const date of dates)for(const time of times)for(const zone of zones)edges.push(date+'T'+time+zone);
for(const suffix of ['\n','\r','\r\n','\u2028','\u2029','\0',' '])edges.push('2024-01-01T00:00:00Z'+suffix);
edges.push('',null,1,'2024-01-01t00:00:00Z','2024-01-01 00:00:00Z');
for(const precision of [null,0,1,3,1000])for(const local of [false,true])for(const offset of [false,true])check(z.string().datetime({precision,local,offset}),edges);
check(z.string().datetime(),edges);
const sample=edges.slice(0,100);
for(const schema of [z.string().datetime().datetime({offset:true}),z.string().datetime({local:true}).datetime({precision:3}),z.string().datetime().min(25),z.string().datetime().startsWith('2024'),z.string().datetime().time(),z.string().date().datetime(),z.string().datetime().date()])check(schema,sample);
check(z.object({value:z.string().datetime().optional()}),[{},...sample.map(value=>({value}))]);
check(z.array(z.string().datetime()),sample.map(value=>[value]));
check(z.union([z.string().datetime(),z.literal('alternate')]),[...sample,'alternate']);
for(const precision of [-1,0.5,1001,Infinity,NaN])assert.throws(()=>validator.openapi2020(z.string().datetime({precision}),{zodChecks:true}),/precision/);
for(const options of [{local:1},{offset:'yes'}])assert.throws(()=>validator.openapi2020(z.string().datetime(options),{zodChecks:true}),/booleans/);
for(const format of ['uuid',undefined,null]){const out=validator.openapi2020(z.string().datetime().openapi({format}),{zodChecks:true});assert.equal(out.format,format);assert.equal(out['x-forklaunch-zod-checks'],undefined);}
assert.throws(()=>validator.openapi2020(z.string().trim().datetime(),{zodChecks:true}),/transformations/);
for(const rule of [null,{},[],{precision:null,local:false},{precision:0,local:false,offset:1},{precision:-1,local:false,offset:false},{precision:1001,local:false,offset:false},{precision:null,local:false,offset:false,extra:true}]){
 const schema={type:'string','x-forklaunch-zod-checks':{version:1,dateTimes:[rule]}};
 assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(schema),/datetime rules/);
 if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(schema)+'\n"2024-01-01T00:00Z"\n'}));
}
const combined={type:'string','x-forklaunch-zod-checks':{version:1,dateTimes:Array(5001).fill({precision:null,local:false,offset:false}),timePrecisions:Array(5000).fill(null)}};
assert.throws(()=>registerZodChecks(new Ajv({strict:false})).compile(combined),/10000 rules/);
if(native)assert.throws(()=>run(path.join(target,'debug/examples/uri_schema_parity'),[],{input:JSON.stringify(combined)+'\n"2024-01-01T00:00Z"\n'}));
console.log(`PASS ${total} actual Zod/AJV datetime comparisons and ${nativeCount} native observations; bounded precision, local/offset options, lexical offsets and invalid-profile admission`);
