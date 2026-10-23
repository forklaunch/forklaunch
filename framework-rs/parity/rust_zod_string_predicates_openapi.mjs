// Exact input assertion projection; transforms and untyped JS position coercions
// remain separate. Native values are valid UTF8; needle units may be surrogates.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES+'/package.json');
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default;
const {registerZodChecks}=load(root+'/framework/universal-sdk/src/core/zodChecks.ts');
const values=['','a','ab','ba','aba','😀','a😀','😀a','x😀y','a\nb','a\n','aa','.',null,0,[],{}];
const needles=['','a','b','ab','😀','\ud83d','\ude00','a\n','.','[\\$'];
const positions=[undefined,NaN,-Infinity,-2,-.5,-0,0,.5,1,1.5,2,2.5,3,Infinity,Number.MAX_VALUE];
const rows=[];let portable=0;
function check(source,inputs=values){
 const expected=inputs.map(value=>validator.parse(source,value).ok);
 for(const pureInput of [false,true]){
  const schema=JSON.parse(JSON.stringify(validator.openapi2020(source,{pureInput,zodChecks:true})));
  const validate=registerZodChecks(new Ajv({strict:false})).compile(schema);
  assert.deepEqual(inputs.map(value=>validate(value)),expected,JSON.stringify(schema));
  rows.push({schema,values:inputs,expected:{compiled:true,valid:expected}});
 }
}
for(const kind of ['startsWith','endsWith','includes'])for(const needle of needles)for(const position of kind==='includes'?positions:[undefined]){
 const source=kind==='includes'?z.string().includes(needle,{position}):z.string()[kind](needle);check(source);
 if(!/[\ud800-\udfff]/u.test(needle) && (kind!=='includes'||needle===''||position===undefined||position===0)){
  assert.deepEqual(validator.openapi2020(source,{zodChecks:true}),validator.openapi2020(source));portable++;
 }
}
for(const wrap of [x=>z.object({value:x}),x=>z.array(x),x=>x.optional(),x=>x.nullable(),x=>x.default('😀'),x=>z.union([x,z.number()]),x=>z.intersection(x,z.string().min(1))]){
 for(const source of [z.string().includes('a',{position:1.5}),z.string().startsWith('\ud83d'),z.string().endsWith('\ude00')]){
  const wrapped=wrap(source);const type=wrapped._def.typeName;
  check(wrapped,values.map(value=>type==='ZodObject'?{value}:type==='ZodArray'?[value]:value));
 }
}
check(z.string().includes('a',{position:1}).includes('b',{position:2}).min(2.5).max(8));
check(z.string().startsWith('\ud83d').endsWith('\ude00').regex(/./u));
let overrides=0;
for(const allOf of [undefined,null,[],[{maxLength:1}]]){
 const source=z.string().includes('a',{position:1}).openapi({allOf});
 const output=validator.openapi2020(source,{pureInput:true,zodChecks:true});
 if(allOf===undefined||allOf===null)assert.equal(output.allOf,allOf);
 else {assert.equal(output.allOf.length,1+allOf.length);assert.deepEqual(output.allOf.slice(1),allOf);}
 overrides++;
}
const pattern=validator.openapi2020(z.string().includes('a',{position:1}).openapi({pattern:'b'}),{zodChecks:true});assert.equal(pattern.pattern,'b');assert.equal(pattern.allOf.length,1);overrides++;
const controls=[];
function control(profile,ok){const schema={'x-forklaunch-zod-checks':profile};let actual;try{const validate=registerZodChecks(new Ajv({strict:false})).compile(schema);actual={compiled:true,valid:values.map(value=>validate(value))};}catch{actual={compiled:false};}assert.equal(actual.compiled,ok);controls.push({schema,values,expected:actual});}
const predicate={kind:'includes',sourceUnits:[97],position:1};
for(const rule of [null,{}, {...predicate,kind:'regex'},{...predicate,sourceUnits:[-1]},{...predicate,sourceUnits:[65536]},{...predicate,sourceUnits:[.5]},{...predicate,sourceUnits:['97']},{...predicate,sourceUnits:[null]},{...predicate,position:null},{...predicate,position:'nan'},{...predicate,extra:0},{kind:'startsWith',sourceUnits:[97],position:0},{...predicate,sourceUnits:Array(4097).fill(97)}])control({version:1,stringPredicates:[rule]},false);
control({version:1,stringPredicates:Array(65).fill({...predicate,sourceUnits:Array(4096).fill(97)})},false);
control({version:1,stringPredicates:Array(64).fill({...predicate,sourceUnits:Array(4096).fill(97)})},true);
controls.at(-1).nativeResourceBoundary = true;
control({version:1,stringPredicates:Array(5001).fill(predicate),utf16MinLengths:Array(5000).fill(0)},false);
control({version:1,stringPredicates:[{kind:'startsWith',sourceUnits:[0xd83d]},{kind:'endsWith',sourceUnits:[0xde00]}]},true);
for(const source of [z.string().includes('a',{position:'1'}),z.string().includes('a',{position:null}),z.string().includes('a',{position:true})])assert.throws(()=>validator.openapi2020(source,{zodChecks:true}),/position/);
for(const method of ['trim','toLowerCase','toUpperCase'])assert.throws(()=>validator.openapi2020(z.string()[method]().includes('a',{position:1}),{zodChecks:true}),/transformations/);
assert.throws(()=>validator.openapi2020(z.string().includes('x'.repeat(4097),{position:1}),{zodChecks:true}),/source unit/);
const excessive=new z.ZodString({...z.string()._def,checks:[...Array(5000).fill({kind:'length',value:1.5}),{kind:'includes',value:'a',position:1}]});assert.throws(()=>validator.openapi2020(excessive,{zodChecks:true}),/10000 rules/);
let callbacks=0;validator.openapi2020(z.object({value:z.string().includes('a',{position:1}).default(()=>{callbacks++;throw Error('callback');})}),{pureInput:true,zodChecks:true});assert.equal(callbacks,0);
const mutable={version:1,stringPredicates:[{...predicate,sourceUnits:[97]}]};const ajv=new Ajv({strict:false,coerceTypes:false,allErrors:true});const frozen=registerZodChecks(ajv).compile({'x-forklaunch-zod-checks':mutable});mutable.stringPredicates[0].sourceUnits[0]=98;mutable.stringPredicates[0].position=0;assert.equal(frozen('aa'),true);assert.equal(frozen('b'),false);assert.equal(ajv.opts.allErrors,true);assert.equal(ajv.opts.coerceTypes,false);
let native=false;
if(process.env.FORKLAUNCH_STRING_PREDICATES_NATIVE==='1'){
 const features=process.env.FORKLAUNCH_STRING_PREDICATES_ARBITRARY==='1'?'zod-checks,jsonschema/arbitrary-precision':'zod-checks';const all=[...rows,...controls];
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features',features,'--example','array_cardinality_parity'],{input:all.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});assert.equal(run.status,0,run.stderr);const result=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(result.length,all.length);
 all.forEach((row,i)=>{if(row.nativeResourceBoundary)assert.deepEqual(result[i],{compiled:false,error:'JSON schema exceeds node limit'});else if(row.expected.compiled)assert.deepEqual(result[i],row.expected,`row ${i}`);else assert.equal(result[i].compiled,false);});native=true;
}
console.log(`PASS ${rows.reduce((n,row)=>n+row.values.length,0)} actualZod/AJV${native?'/native':''} cases/${rows.length} schemas, ${portable} portable artifact controls, ${overrides} annotation controls, ${controls.length} admission/resource controls (1 explicit native global-node-limit boundary), callbacks=${callbacks}`);
