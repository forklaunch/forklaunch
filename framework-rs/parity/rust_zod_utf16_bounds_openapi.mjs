// Extended bound registrations reuse UTF16 counting; Rust input is UTF8 strings.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default;
const {registerZodChecks}=load(root+'/framework/universal-sdk/src/core/zodChecks.ts');
const bounds=[NaN,-Infinity,Infinity,-Number.MAX_VALUE,-1.5,-1,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,.5,1,1.5,2,2.5,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,Number.MAX_VALUE];
const values=['','a','ab','😀','a😀','😀😀',null,0,[],{}];
const suffixes=[null,['min',1],['max',2],['length',2]];
const baseline=[];
for(const method of ['min','max','length'])for(const bound of [0,1,2,Number.MAX_SAFE_INTEGER])for(const suffix of suffixes){let source=z.string()[method](bound);if(suffix)source=source[suffix[0]](suffix[1]);baseline.push(validator.openapi2020(source,{pureInput:true,zodChecks:true}));}
assert.equal(createHash('sha256').update(JSON.stringify(baseline)).digest('hex'),'48acc2b72250d0d428a1a054462a5e2c79b421f672ead9a6ea30f5f069de91fb','safe-integer profile artifacts changed');
const rows=[];
function check(source,inputs=values){
 const expected=inputs.map(value=>validator.parse(source,value).ok);
 for(const pureInput of [false,true]){
  const schema=JSON.parse(JSON.stringify(validator.openapi2020(source,{pureInput,zodChecks:true})));
  const validate=registerZodChecks(new Ajv({strict:false})).compile(schema);assert.deepEqual(inputs.map(value=>validate(value)),expected,JSON.stringify(schema));
  rows.push({schema,values:inputs,ajv:{compiled:true,valid:expected}});
 }
}
for(const method of ['min','max','length'])for(const bound of bounds)for(const suffix of suffixes){let source=z.string()[method](bound);if(suffix)source=source[suffix[0]](suffix[1]);check(source);}
for(const [wrap,wrapValue]of [[x=>z.object({text:x}),x=>({text:x})],[x=>z.array(x),x=>[x]],[x=>z.union([x,z.number()]),x=>x],[x=>z.intersection(x,z.string().min(1)),x=>x],[x=>x.nullable(),x=>x],[x=>x.optional(),x=>x],[x=>x.default('ab'),x=>x]])for(const source of [z.string().min(1.5).max(2.5),z.string().length(NaN),z.string().length(Infinity),z.string().max(-.5),z.string().min(-Infinity)])check(wrap(source),values.map(wrapValue));
let overrides=0;
for(const minLength of [undefined,null,0,2])for(const maxLength of [undefined,null,1,3]){
 const schema=validator.openapi2020(z.string().length(1.5).openapi({minLength,maxLength}),{zodChecks:true});assert.equal(schema['x-forklaunch-zod-checks'],undefined);assert.equal(schema.minLength,minLength);assert.equal(schema.maxLength,maxLength);overrides++;
}
for(const [key,other]of [['minLength','utf16MaxLengths'],['maxLength','utf16MinLengths']])for(const value of [undefined,null,0,2]){
 const schema=validator.openapi2020(z.string().length(NaN).openapi({[key]:value}),{zodChecks:true});assert.deepEqual(schema['x-forklaunch-zod-checks'],{version:1,[other]:['NaN']});assert.equal(schema[key],value);overrides++;
}
const ordered=validator.openapi2020(z.string().min(-1.5).openapi({minLength:1}).openapi({minLength:2}),{zodChecks:true});assert.equal(ordered.minLength,2);assert.equal(ordered['x-forklaunch-zod-checks'],undefined);
let callbacks=0;validator.openapi2020(z.object({text:z.string().min(1.5).default(()=>{callbacks++;throw Error('callback')})}),{pureInput:true,zodChecks:true});assert.equal(callbacks,0);
for(const transform of ['trim','toLowerCase','toUpperCase'])assert.throws(()=>validator.openapi2020(z.string()[transform]().min(1.5),{zodChecks:true}),/transformations/);
assert.throws(()=>validator.openapi2020(z.string().min('2'),{zodChecks:true}),/must be numbers/);
const many=new z.ZodString({...z.string()._def,checks:Array(5001).fill({kind:'length',value:1.5})});assert.throws(()=>validator.openapi2020(many,{zodChecks:true}),/10000 rules/);
const controls=[];
function control(profile,expected){const schema={'x-forklaunch-zod-checks':profile};let result;try{const validate=registerZodChecks(new Ajv({strict:false})).compile(schema);result={compiled:true,valid:values.map(value=>validate(value))};}catch{result={compiled:false};}assert.equal(result.compiled,expected);controls.push({schema,values,ajv:result});return result;}
for(const key of ['utf16MinLengths','utf16MaxLengths'])for(const invalid of [null,0,[null],[true],['nan'],Array(10001).fill(0)])control({version:1,[key]:invalid},false);
for(const key of ['utf16MinLength','utf16MaxLength'])for(const invalid of [-1,.5,'NaN','Infinity',Number.MAX_SAFE_INTEGER+1])control({version:1,[key]:invalid},false);
control({version:1,utf16MinLengths:Array(5001).fill(0),arrayMaxItems:Array(5000).fill(2)},false);
control({version:1,utf16MinLengths:Array(5000).fill(0),arrayMaxItems:Array(5000).fill(2)},true);
const mutable={version:1,utf16MinLengths:[1.5],utf16MaxLengths:[2.5]};const injected=new Ajv({strict:false,allErrors:true,coerceTypes:false});const immutable=registerZodChecks(injected).compile({'x-forklaunch-zod-checks':mutable});mutable.utf16MinLengths[0]=0;mutable.utf16MaxLengths[0]=100;assert.equal(immutable('a'),false);assert.equal(immutable('😀'),true);assert.equal(immutable('a😀'),false);assert.equal(injected.opts.allErrors,true);assert.equal(injected.opts.coerceTypes,false);
// Host JS supports unpaired UTF16 units; these controls are deliberately not
// sent to the Rust JSON example, whose string representation excludes them.
for(const input of ['\ud800','\udfff','\ud800a']){const source=z.string().min(1.5);const schema=validator.openapi2020(source,{zodChecks:true});assert.equal(registerZodChecks(new Ajv({strict:false})).compile(schema)(input),validator.parse(source,input).ok);}
let native=false;
if(process.env.FORKLAUNCH_UTF16_BOUNDS_NATIVE==='1'){
 const features=process.env.FORKLAUNCH_UTF16_BOUNDS_ARBITRARY==='1'?'zod-checks,jsonschema/arbitrary-precision':'zod-checks';const all=[...rows,...controls];
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features',features,'--example','array_cardinality_parity'],{input:all.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});assert.equal(run.status,0,run.stderr);const result=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(result.length,all.length);
 for(let i=0;i<all.length;i++){if(all[i].ajv.compiled)assert.deepEqual(result[i],all[i].ajv,JSON.stringify(all[i].schema));else assert.equal(result[i].compiled,false);};native=true;
}
console.log(`PASS ${rows.reduce((n,row)=>n+row.values.length,0)} exact actualZod/AJV${native?'/native':''} cases across ${rows.length} schemas; ${baseline.length} byte-stable safe profiles, ${overrides} overrides, ${controls.length} admission/budget controls, 3 JS-only lone-surrogate controls and callbacks=${callbacks}`);
