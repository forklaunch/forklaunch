// Finite numeric and nonnegative safe-integer array bounds only. This fixture
// does not claim multipleOf, UTF-16 string lengths, or nonfinite schema parity.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root);
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??'/private/tmp/fl-canonical-http-dependencies','package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv');
const socketRequire=createRequire(path.join(process.env.FORKLAUNCH_TEST_SOCKET_DEPENDENCIES??'/private/tmp/fl-binary-socket-dependencies','package.json'));
let AjvDraft4;try{AjvDraft4=require('ajv-draft-04')}catch(error){if(error.code!=='MODULE_NOT_FOUND')throw error;AjvDraft4=socketRequire('ajv-draft-04')}
const {generateSchema}=load(path.join(root,'framework/validator/shims/zod-v3-openapi/zod-openapi.ts'));
const variants=[['direct',x=>x,x=>x],['optional',x=>x.optional(),x=>x],['nullable',x=>x.nullable(),x=>x],['object',x=>z.object({value:x}),x=>({value:x})],['array',x=>z.array(x),x=>[x]],['nested',x=>z.object({rows:z.array(z.object({value:x}))}),x=>({rows:[{value:x}]})]];
let cases=0,schemas=0;
function check(label,schema,values){
 for(const [shape,wrapSchema,wrap] of variants){
  const wrapped=wrapSchema(schema);
  for(const version of ['3.0','3.1']){
   const exported=version==='3.1'?validator.openapi(wrapped):generateSchema(wrapped,false,version);
   const engine=version==='3.0'?new AjvDraft4({strict:false}):new Ajv({strict:false});
   const validate=engine.compile(exported);schemas++;
   for(const value of values){const input=wrap(value),expected=validator.parse(wrapped,input).ok;assert.equal(validate(input),expected,JSON.stringify({label,shape,version,input,exported}));cases++;}
  }
 }
}
const numericValues=[-1e21,-11,-10,-1,-0,0,0.5,1,2,9,10,11,Number.MAX_SAFE_INTEGER,1e21,null,'1',false,[],{}];
const bounds=[-10,-0,0.5,10,Number.MAX_SAFE_INTEGER,1e20];
const methods=['min','max','gt','lt'];
for(const first of methods)for(const a of bounds){
 check(`${first}${a}`,z.number()[first](a),numericValues);
 for(const second of methods)for(const b of bounds)check(`${first}${a}/${second}${b}`,z.number()[first](a)[second](b),numericValues);
}
const arrayValues=[null,{},'x',...Array.from({length:6},(_,length)=>Array.from({length},()=> 'x'))];
for(const first of ['min','max','length'])for(const a of [0,1,2,3]){
 check(`array${first}${a}`,z.array(z.string())[first](a),arrayValues);
 for(const second of ['min','max','length'])for(const b of [0,1,2,3])check(`array${first}${a}/${second}${b}`,z.array(z.string())[first](a)[second](b),arrayValues);
}
// Triple constraints retain an impossible intersection regardless of order.
for(const order of [['length','min','max'],['max','length','min'],['min','max','length']]){
 let schema=z.array(z.string());for(const method of order)schema=schema[method]({length:2,min:3,max:1}[method]);check(order.join(','),schema,arrayValues);
}
assert.equal(validator.openapi(z.number().min(10).min(0).openapi({minimum:-20})).minimum,-20);
assert.equal(validator.openapi(z.array(z.string()).length(2).min(3).openapi({minItems:0})).minItems,0);
// Runtime declaration order and custom diagnostics remain unchanged.
assert.deepEqual(validator.parse(z.number().min(10,'first').min(5,'second'),0).errors.map(e=>e.message),['first','second']);
let callbacks=0;const callback=()=>{callbacks++;throw Error('export must not invoke callback');};
for(const base of [z.number().min(10).min(0),z.array(z.string()).length(2).min(1)]){
 for(const schema of [base.transform(callback),base.refine(callback),z.preprocess(callback,base),z.object({value:base.transform(callback)})])validator.openapi(schema);
 validator.openapiRequestBody(base.default(callback));
}
assert.equal(callbacks,0);
// These invalid/non-JSON registrations are unchanged, not silently normalized.
assert.equal(validator.openapi(z.number().min(Infinity)).minimum,Infinity);
assert.equal(validator.openapi(z.array(z.string()).length(1.5)).minItems,1.5);
console.log(`PASS ${cases} actual Zod/AJV bound comparisons over ${schemas} OpenAPI3.0/3.1 exports; repeated/tied/exclusive bounds, array exact/min/max contradictions, six shapes, overrides and zero callbacks`);
