// Finite JSON numeric inputs; nonfinite registration values remain exact.
// Nonfinite *inputs*, callbacks and ordinary engine coercion are separate.
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
const originalBounds=[NaN,Infinity,-Infinity,-Number.MAX_VALUE,-1.5,-0,0,1.5,Number.MAX_VALUE];
const bounds=[...originalBounds,-Number.MIN_VALUE,Number.MIN_VALUE,-Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER,2**53,-(2**53)];
const values=[null,'0',false,{},[],-Number.MAX_VALUE,-2,-1.5,-1,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,.5,1,1.5,2,Number.MAX_VALUE];
const baseline=[],rows=[];
const suffixes=[null,['min',-1],['max',1],['gt',0],['lt',0]];
for(const method of ['min','max','gt','lt'])for(const bound of originalBounds)for(const suffix of suffixes){
 let source=z.number()[method](bound);if(suffix)source=source[suffix[0]](suffix[1]);
 baseline.push(JSON.parse(JSON.stringify(validator.openapi2020(source,{pureInput:true}))));
}
assert.equal(createHash('sha256').update(JSON.stringify(baseline)).digest('hex'),'0259e2c082a62cec4ce2065676d379826efa6df2a0898b717ba5ee2d11689075','baseline exports changed');
const check=(source,inputs=values)=>{
 const expected=inputs.map(value=>validator.parse(source,value).ok);
 for(const pureInput of [false,true]){
  const schema=JSON.parse(JSON.stringify(validator.openapi2020(source,{pureInput,zodChecks:true})));
  const validate=registerZodChecks(new Ajv({strict:false,coerceTypes:false})).compile(schema);
  assert.deepEqual(inputs.map(value=>validate(value)),expected,JSON.stringify(schema));
  rows.push({schema,values:inputs,ajv:{compiled:true,valid:expected}});
 }
};
for(const method of ['min','max','gt','lt'])for(const bound of bounds)for(const suffix of suffixes){
 let source=z.number()[method](bound);if(suffix)source=source[suffix[0]](suffix[1]);check(source);
}
for(const [wrap,wrapValue]of [[x=>z.object({number:x}),x=>({number:x})],[x=>z.array(x),x=>[x]],[x=>z.union([x,z.string()]),x=>x],[x=>z.intersection(x,z.number().min(-1)),x=>x],[x=>x.nullable(),x=>x],[x=>x.optional(),x=>x],[x=>x.default(0),x=>x]])for(const source of [z.number().min(Infinity).min(-1),z.number().max(-Infinity).max(1),z.number().min(NaN).gt(-1),z.number().max(Infinity).multipleOf(.1),z.number().min(-Infinity).int()])check(wrap(source),values.map(wrapValue));
// Own annotations suppress only the corresponding source bound family.
const keywords={min:'minimum',max:'maximum',gt:'exclusiveMinimum',lt:'exclusiveMaximum'};
let overrides=0;
for(const [method,keyword]of Object.entries(keywords))for(const bound of [Infinity,-Infinity,NaN,2])for(const override of [undefined,null,0,2]){
 const source=z.number()[method](bound).openapi({[keyword]:override});
 const schema=validator.openapi2020(source,{zodChecks:true});assert.equal(schema[keyword],override);assert.equal(schema.allOf,undefined);overrides++;
}
// Distinct inclusive/exclusive keywords keep their individual constraints.
for(const [source,keyword,other,value] of [
 [z.number().min(5).gt(2),'minimum','exclusiveMinimum',2],
 [z.number().min(2).gt(5),'exclusiveMinimum','minimum',2],
 [z.number().max(5).lt(2),'exclusiveMaximum','maximum',5],
 [z.number().max(2).lt(5),'maximum','exclusiveMaximum',5]
]){const schema=validator.openapi2020(source.openapi({[keyword]:undefined}),{zodChecks:true});assert.equal(schema[keyword],undefined);assert.equal(schema[other],value);}
const ordered=validator.openapi2020(z.number().min(Infinity).openapi({minimum:1}).openapi({minimum:2}),{zodChecks:true});assert.equal(ordered.minimum,2);assert.equal(ordered.allOf,undefined);
const contradiction=validator.openapi2020(z.number().min(Infinity).openapi({not:{type:'string'},allOf:[{minimum:-1}]}),{zodChecks:true});const validate=new Ajv({strict:false}).compile(contradiction);assert.equal(validate(0),false);
assert.throws(()=>validator.openapi2020(z.number().min('2'),{zodChecks:true}),/must be numbers/);
let callbacks=0;validator.openapi2020(z.object({number:z.number().min(Infinity).default(()=>{callbacks++;throw Error('default invoked')})}),{pureInput:true,zodChecks:true});assert.equal(callbacks,0);
let native=false;
if(process.env.FORKLAUNCH_NUMBER_BOUNDS_NATIVE==='1'){
 const features=process.env.FORKLAUNCH_NUMBER_BOUNDS_ARBITRARY==='1'?'zod-checks,jsonschema/arbitrary-precision':'zod-checks';
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features',features,'--example','array_cardinality_parity'],{input:rows.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(run.status,0,run.stderr);const result=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(result.length,rows.length);
 for(let i=0;i<rows.length;i++)assert.deepEqual(result[i],rows[i].ajv,JSON.stringify(rows[i].schema));native=true;
}
console.log(`PASS ${rows.reduce((n,row)=>n+row.values.length,0)} actual Zod/AJV${native?'/native':''} comparisons over ${rows.length} schemas; ${baseline.length} byte-pinned baseline exports, ${overrides} own override controls, separate inclusive/exclusive constraints and callback count=${callbacks}`);
