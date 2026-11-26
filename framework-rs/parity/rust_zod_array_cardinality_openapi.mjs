// Actual exporter characterization and opt-in standard-keyword candidate probe.
// Baseline output remains intentionally unchanged; nonfinite values serialize
// to null and can invalidate schemas. Strings/UTF16/transforms are separate.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {writeFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const {registerZodChecks}=load(path.join(root,'framework/universal-sdk/src/core/zodChecks.ts'));
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv2020=require('ajv/dist/2020').default;
const socketRequire=createRequire(path.join(process.env.FORKLAUNCH_TEST_SOCKET_DEPENDENCIES??'/private/tmp/fl-binary-socket-dependencies','package.json'));
let AjvDraft4;try{AjvDraft4=require('ajv-draft-04')}catch(error){if(error.code!=='MODULE_NOT_FOUND')throw error;AjvDraft4=socketRequire('ajv-draft-04')}
const bounds=[-Infinity,-Number.MAX_VALUE,-2,-0.5,-0,0,0.5,1,1.5,2,2.5,4294967296,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,18446744073709551616,1e21,1e100,Number.MAX_VALUE,Infinity,NaN];
const values=[[],['x'],['x','y'],['x','y','z'],null,{},'x',[0]];
const profiles=[['default',{}],['pure',{pureInput:true}]];
const own=(value,key)=>Object.prototype.hasOwnProperty.call(value,key);
const describe=value=>typeof value==='number'&&!Number.isFinite(value)?String(value):value;
function compile(schema){try{const Engine=schema.$schema==='http://json-schema.org/draft-04/schema#'?AjvDraft4:Ajv2020;const validate=registerZodChecks(new Engine({strict:false})).compile(schema);return {compiled:true,valid:values.map(value=>validate(value))}}catch(error){return {compiled:false,error:error.message}}}
const observations=[],candidates=[];
for(const method of ['min','max','length'])for(const bound of bounds)for(const [profile,options]of profiles){
 const schema=z.array(z.string())[method](bound),exported=validator.openapi2020(schema,options);
 const baseline={type:['array'],items:{type:['string']},...(method!=='max'?{minItems:bound}:{}),...(method!=='min'?{maxItems:bound}:{}),$schema:'https://json-schema.org/draft/2020-12/schema'};
 assert.equal(JSON.stringify(exported),JSON.stringify(baseline),'baseline serialized output changed');
 const wire=JSON.parse(JSON.stringify(exported));
 const expected=values.map(value=>validator.parse(schema,value).ok);
 const direct=compile(exported),serialized=compile(wire);
 // Characterize, do not bless baseline invalid artifacts as parity successes.
 if(Number.isFinite(bound)&&bound>=0&&Number.isInteger(bound)&&bound<=Number.MAX_SAFE_INTEGER)assert.deepEqual(serialized.valid,expected);
 if(Number.isFinite(bound)&&(!Number.isInteger(bound)||bound<0))assert.equal(serialized.compiled,false);
 if(!Number.isFinite(bound))assert.equal(serialized.compiled,false);
 observations.push({method,bound:describe(bound),profile,exported:JSON.parse(JSON.stringify(exported,(_,value)=>describe(value))),wire,direct,serialized,expected});
}
// Current own-key overrides: null intentionally invalid, valid numeric explicit
// values supersede source bounds, undefined removes keyword in explicit profile.
for(const override of [undefined,null,0,2])for(const [profile,options]of profiles){
 const schema=z.array(z.string()).min(1.5).openapi({minItems:override});
 const exported=validator.openapi2020(schema,options),wire=JSON.parse(JSON.stringify(exported));
 if(override!==undefined)assert.equal(exported.minItems,override);
 else if(profile!=='default')assert.equal(own(wire,'minItems'),false);
 observations.push({override:override===undefined?'undefined':override,profile,wire,serialized:compile(wire)});
}
// Candidate normalized standard cardinalities only. Native compilation admission
// is recorded separately: large integer support must be proven before selection.
for(const dialect of ['http://json-schema.org/draft-04/schema#','https://json-schema.org/draft/2020-12/schema'])for(const keyword of ['minItems','maxItems'])for(const bound of [0,1,2,3,4294967296,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,18446744073709551616,1e21,1e100,Number.MAX_VALUE]){
 const schema={$schema:dialect,type:'array',items:{type:'string'},[keyword]:bound};
 candidates.push({schema,values,ajv:compile(schema)});
}
// Opted-in output keeps IEEE tags exact and never emits invalid standard bounds.
const projected=[];
const make=(method,bound)=>z.array(z.string())[method](bound);
for(const first of ['min','max','length'])for(const bound of bounds)for(const pureInput of [false,true]) {
 for(const second of [undefined,'min','max','length']) {
  let source=make(first,bound);if(second)source=source[second](1.5);
  for(const dialect of ['http://json-schema.org/draft-04/schema#','https://json-schema.org/draft/2020-12/schema']) {
   const schema=validator.openapi2020(source,{pureInput,zodChecks:true});schema.$schema=dialect;
   const wire=JSON.parse(JSON.stringify(schema)),result=compile(wire),expected=values.map(value=>validator.parse(source,value).ok);
   assert.equal(result.compiled,true,result.error);assert.deepEqual(result.valid,expected);
   projected.push({schema:wire,values,ajv:result});
  }
 }
}
for(const [wrap,values] of [
 [x=>z.object({rows:x}),[{rows:[]},{rows:['x','y']},{rows:[0]},{}]],
 [x=>z.array(x),[[],[[]],[['x','y']]]],
 [x=>x.optional(),[[],['x','y'],null]],
 [x=>x.nullable(),[[],['x','y'],null]],
 [x=>z.union([x,z.string()]),[[],['x','y'],'other']],
 [x=>z.intersection(x,z.array(z.string()).max(2.5)),[[],['x','y'],['a','b','c']]]
]) {
 const source=wrap(z.array(z.string()).min(1.5).length(NaN));
 const schema=JSON.parse(JSON.stringify(validator.openapi2020(source,{pureInput:true,zodChecks:true})));
 const validate=registerZodChecks(new Ajv2020({strict:false})).compile(schema);
 const result={compiled:true,valid:values.map(value=>validate(value))};assert.deepEqual(result.valid,values.map(value=>validator.parse(source,value).ok));
 projected.push({schema,values,ajv:result});
}
let callbacks=0;const callback=()=>{callbacks++;throw Error('must not execute')};
validator.openapi2020(z.object({rows:z.array(z.string()).min(1.5).default(callback)}),{pureInput:true,zodChecks:true});assert.equal(callbacks,0);
for(const minItems of [undefined,null,0,2])for(const maxItems of [undefined,null,1,3]) {
 const source=z.array(z.string()).length(1.5).openapi({minItems,maxItems});
 const schema=validator.openapi2020(source,{zodChecks:true});
 assert.equal(own(schema,'x-forklaunch-zod-checks'),false);
 assert.equal(schema.minItems,minItems);assert.equal(schema.maxItems,maxItems);
}
for(const [key,other] of [['minItems','arrayMaxItems'],['maxItems','arrayMinItems']])for(const value of [undefined,null,0,2]) {
 const source=z.array(z.string()).length(1.5).openapi({[key]:value});
 const schema=validator.openapi2020(source,{zodChecks:true});
 assert.deepEqual(schema['x-forklaunch-zod-checks'],{version:1,[other]:[1.5]});
 assert.equal(schema[key],value);
}
const ordered=validator.openapi2020(z.array(z.string()).min(1.5).openapi({minItems:0}).openapi({minItems:2}),{zodChecks:true});
assert.equal(ordered.minItems,2);assert.equal(own(ordered,'x-forklaunch-zod-checks'),false);
for(const keyword of ['arrayMinItems','arrayMaxItems'])for(const value of [null,{},[null],['nan'],[true],Array(10001).fill(1)]) {
 const schema={'x-forklaunch-zod-checks':{version:1,[keyword]:value}};
 assert.equal(compile(schema).compiled,false);
}
assert.equal(compile({'x-forklaunch-zod-checks':{version:1,arrayMinItems:Array(5001).fill(0),arrayMaxItems:Array(5000).fill(2)}}).compiled,false);
assert.throws(()=>validator.openapi2020(z.array(z.string()).openapi({'x-forklaunch-zod-checks':{version:1}}),{zodChecks:true}),/reserved/i);
// Injected engines retain their options; admitted numeric arrays are copied.
const injected=new Ajv2020({strict:false,allErrors:true,coerceTypes:false});registerZodChecks(injected);
const mutable={version:1,arrayMinItems:[2]};const validate=injected.compile({type:'array','x-forklaunch-zod-checks':mutable});mutable.arrayMinItems[0]=0;
assert.equal(validate([]),false);assert.equal(injected.opts.allErrors,true);assert.equal(injected.opts.coerceTypes,false);
const baselineCandidates=candidates.length;
candidates.push(...projected);
let native;
if(process.env.FORKLAUNCH_CARDINALITY_PROBE_NATIVE==='1'){
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features',process.env.FORKLAUNCH_CARDINALITY_PROBE_FEATURES==='arbitrary'?'zod-checks,jsonschema/arbitrary-precision':'zod-checks','--example','array_cardinality_parity'],{input:candidates.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(run.status,0,run.stderr);native=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(native.length,candidates.length);
 for(let index=0;index<baselineCandidates;index++) {
  const candidate=candidates[index],bound=candidate.schema.minItems??candidate.schema.maxItems;
  const known=process.env.FORKLAUNCH_CARDINALITY_PROBE_FEATURES==='arbitrary' && candidate.schema.$schema==='http://json-schema.org/draft-04/schema#' && bound>=1e21;
  if(known){assert.equal(native[index].compiled,false);assert.match(native[index].error,/integer/)}
  else assert.deepEqual(native[index],candidate.ajv,JSON.stringify(candidate.schema));
 }
 for(let index=baselineCandidates;index<candidates.length;index++)assert.deepEqual(native[index],candidates[index].ajv,JSON.stringify(candidates[index].schema));
}
const result={observations,candidates:candidates.map((row,index)=>({...row,...(native?{native:native[index]}:{})}))};
if(process.env.FORKLAUNCH_CARDINALITY_PROBE_OUTPUT)writeFileSync(process.env.FORKLAUNCH_CARDINALITY_PROBE_OUTPUT,JSON.stringify(result,null,2)+'\n');
console.log(`PASS ${projected.reduce((count,row)=>count+row.values.length,0)} exact instance comparisons over ${projected.length} schemas; ${observations.length} baseline cardinality/override characterizations; ${baselineCandidates} standard-cardinality probes and ${projected.length} exact-profile ${native?'native + ':''}AJV probes (admission recorded separately, no baseline parity claim)`);
if(native)for(let index=0;index<baselineCandidates;index++)console.log(JSON.stringify(result.candidates[index]));
