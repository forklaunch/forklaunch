import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default;
const {registerZodChecks}=load(root+'/framework/universal-sdk/src/core/zodChecks.ts');
const options={pureInput:true,zodChecks:true};
let callbacks=0;const callback=()=>{callbacks++;throw Error('callback executed')};
const keys=[['min',z.string().min(2)],['max',z.string().max(1)],['starts',z.string().startsWith('a')],['email',z.string().email()],['uuid',z.string().uuid()],['optional',z.string().min(2).optional()],['nullable',z.string().min(2).nullable()],['default',z.string().min(2).default(callback)],['union',z.union([z.string().startsWith('a'),z.literal('b')])],['enum',z.enum(['a','b'])],['nativeEnum',z.nativeEnum({A:'a',B:'b'})],['mixedNativeEnum',z.nativeEnum({0:'Zero',Zero:0,A:'a'})],['boolean',z.boolean()],['null',z.null()],['literalNumber',z.literal(1)],['array',z.array(z.string())],['object',z.object({})],['intersection',z.intersection(z.string().min(2),z.string().max(3))],['nullish',z.string().min(2).nullish()],['readonly',z.string().min(2).readonly()],['branded',z.string().min(2).brand()],['number',z.number()],['never',z.never()],['unknown',z.unknown()]];
const names=['','a','aa','b','😀','a😀','constructor','__proto__','a@b.co','a%b@b.co','a\nb','FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF'];
const values=[{},...names.map(name=>Object.fromEntries([[name,1]])),...names.map(name=>Object.fromEntries([[name,'bad']]))];
const rows=[];
const {UniversalSdk}=load(root+'/framework/universal-sdk/src/universalSdk.ts');
const owned=await UniversalSdk.create('https://example.invalid',{});
const injected=new Ajv({strict:false,ownProperties:true,coerceTypes:false,allErrors:true});
const custom=new UniversalSdk('https://example.invalid',injected,{});
const coercing=new Ajv({strict:false,ownProperties:true,coerceTypes:true,allErrors:true});
const customCoercing=new UniversalSdk('https://example.invalid',coercing,{});
for(const [name,key]of keys)for(const [wrap,wrapValue] of [
 [x=>x,x=>x], [x=>z.object({record:x}),x=>({record:x})],
 [x=>z.array(x),x=>[x]], [x=>z.union([x,z.string()]),x=>x]
]) {
 const source=wrap(statics.record(key,z.number()));
 const schema=validator.openapi2020(source,options);
 const engine=registerZodChecks(new Ajv({strict:false,ownProperties:true,coerceTypes:false}));
 const projected=engine.compile(schema);
 const document={openapi:'3.1.0',jsonSchemaDialect:'https://json-schema.org/draft/2020-12/schema',info:{title:'record',version:'1'},paths:{},components:{schemas:{Target:schema}}};
 const consumers=[owned,custom,customCoercing].map(sdk=>sdk.responseSchemas.validator(document,'/components/schemas/Target',{$ref:'#/components/schemas/Target'}));
 const inputs=values.map(wrapValue), expected=inputs.map(value=>validator.parse(source,value).ok);
 assert.deepEqual(inputs.map(value=>projected(value)),expected,name);
 // Default SDK numeric coercion is a distinct existing policy; key constraints
 // are checked on valid numeric values. Injected no-coercion checks every row.
 assert.deepEqual(inputs.map(value=>consumers[1](value)),expected,name);
 for(let index=0;index<=names.length;index++)for(const consumer of [consumers[0],consumers[2]]) {
  const before=JSON.stringify(inputs[index]);
  assert.equal(consumer(inputs[index]),expected[index],name);
  assert.equal(JSON.stringify(inputs[index]),before,'key validation mutated input');
 }
 rows.push({schema,values:inputs,ajv:{compiled:true,valid:expected}});
}
assert.equal(coercing.opts.coerceTypes,true);
assert.equal(injected.opts.coerceTypes,false);assert.equal(injected.opts.allErrors,true);
// Explicit annotations remain user-owned, including undefined and false.
for(const override of [undefined,null,false,true,{pattern:'^z'}]) {
 const schema=validator.openapi2020(z.record(z.string().min(2),z.number()).openapi({propertyNames:override}),options);
 if(override && typeof override==='object') {
  // Existing schema merge combines the explicit constraint with generated ones.
  assert.equal(schema.propertyNames.pattern,'^z');
 } else assert.equal(schema.propertyNames,override);
}
assert.throws(()=>validator.openapi2020(z.record(z.string().openapi({$ref:'#/$defs/Key'}),z.number()),options),/references/);
for(const type of [null,42,'bogus'])assert.throws(()=>validator.openapi2020(z.record(z.string().openapi({type}),z.number()),options),/type annotation/);
const ordinary=z.record(z.string().min(2),z.number());
assert.equal(validator.openapi2020(ordinary).propertyNames,undefined);
assert.equal(validator.openapi2020(ordinary,{zodChecks:true}).propertyNames,undefined);
const {generateSchema}=load(root+'/framework/validator/shims/zod-v3-openapi/zod-openapi.ts');
for(const dialect of ['3.0','3.1'])assert.equal(generateSchema(ordinary,false,dialect).propertyNames,undefined);
const rejects=[];
for(const [name,key]of [['trim',z.string().trim().min(2)],['upper',z.string().toUpperCase()],['effect',z.string().transform(callback)],['coerce',z.coerce.number()]]){try{validator.openapi2020(statics.record(key,z.number()),options);rejects.push({name,rejected:false})}catch(error){rejects.push({name,rejected:true,error:error.message})}}
assert.equal(callbacks,0);
assert.equal(rejects.every(row=>row.rejected),true);
let native=false;
if(process.env.FORKLAUNCH_RECORD_PURE_NATIVE==='1') {
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','zod-checks','--example','array_cardinality_parity'],{input:rows.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(run.status,0,run.stderr);
 const results=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(results.length,rows.length);
 for(let index=0;index<rows.length;index++)assert.deepEqual(results[index],rows[index].ajv,JSON.stringify(rows[index].schema));
 native=true;
}
console.log(`PASS ${rows.reduce((n,row)=>n+row.values.length,0)} actual Zod/AJV2020/${native?'native/':''}injected SDK comparisons across ${rows.length} schemas; default SDK key-admission controls, explicit annotation/legacy controls and ${rejects.length} pure rejections; callbacks=${callbacks}`);
