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
// Never's finite JSON acceptance is always false. Optional absence remains
// distinct from JSON null; generated models do not yet expose a precise never type.
const values=[null,false,true,0,1,'', 'x',[],[null],['x'],{}, {value:null},{value:'x'},{other:1}];
const cases=[z.never(),z.never().optional(),z.object({value:z.never()}),z.object({value:z.never().optional()}),z.array(z.never()),z.union([z.never(),z.string()]),z.union([z.never(),z.never()]),z.object({values:z.array(z.never())})];
let comparisons=0,exports=0;
for(const schema of cases)for(const version of ['3.0','3.1']){
 const exported=generateSchema(schema,false,version);
 const validate=(version==='3.0'?new AjvDraft4({strict:false}):new Ajv({strict:false})).compile(exported);exports++;
 for(const value of values){assert.equal(validate(value),validator.parse(schema,value).ok,JSON.stringify({version,value,exported}));comparisons++;}
}
assert.equal(validator.parse(z.object({value:z.never().optional()}),{}).ok,true);
assert.equal(validator.parse(z.object({value:z.never()}),{}).ok,false);
assert.equal(validator.parse(z.never().optional(),undefined).ok,true);
for(const version of ['3.0','3.1']){
 const overridden=generateSchema(z.never().openapi({not:{type:'string'},description:'explicit'}),false,version);
 assert.equal(JSON.stringify(overridden),JSON.stringify({not:{type:'string'},description:'explicit'}));
 const validate=(version==='3.0'?new AjvDraft4({strict:false}):new Ajv({strict:false})).compile(overridden);
 assert.equal(validate(1),true);assert.equal(validate('x'),false); // Explicit metadata owns this override.
 const discriminator=generateSchema(z.discriminatedUnion('kind',[z.object({kind:z.literal('a')}),z.object({kind:z.literal('b')})]),false,version);
 assert.ok(discriminator.oneOf);assert.equal(discriminator.anyOf,undefined);
}
let callbacks=0;const callback=()=>{callbacks++;throw Error('export callback')};
for(const schema of [z.never().transform(callback),z.never().refine(callback),z.preprocess(callback,z.never())])validator.openapi(schema);
validator.openapiRequestBody(z.never().default(callback));assert.equal(callbacks,0);
console.log(`PASS ${comparisons} actual Zod/AJV Never comparisons over ${exports} OAS3.0/3.1 exports; required/optional absence, arrays/unions, explicit overrides, discriminator controls and zero callbacks`);
