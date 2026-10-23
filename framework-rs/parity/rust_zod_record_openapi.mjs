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

const {merge}=load(path.join(root,'framework/validator/shims/zod-v3-openapi/schemaMerge.ts'));
const sets=[[],['a'],['a','b','a'],['__proto__','constructor','prototype','toString','forklaunchProperty0','😀','a"b',''],['0','01','4294967295']];
const values=[{},JSON.parse('{"__proto__":1}'),JSON.parse('{"__proto__":"bad"}'),{constructor:1,prototype:2},{toString:3},{a:1},{b:2},{a:'bad'},{other:1},[],null];
let comparisons=0,exports=0,consumerExceptions=0;
for(const keys of sets){
 const base=validator.enum_(Object.fromEntries(keys.map((key,i)=>[i,key])));
 for(const key of [base,base.optional(),base.nullable(),base.optional().nullable()]){
  const record=z.record(key,z.number());
  for(const [schema,wrap]of [[record,x=>x],[z.array(record),x=>[x]],[z.object({value:record}),x=>({value:x})],[z.union([record,z.string()]),x=>x]]){
   for(const version of ['3.0','3.1']){
    const exported=generateSchema(schema,false,version);exports++;
    const validate=(version==='3.0'?new AjvDraft4({strict:false,ownProperties:true}):new Ajv({strict:false,ownProperties:true})).compile(exported);
    for(const value of values){const input=wrap(value);const actual=validate(input),expected=validator.parse(schema,input).ok;if(version==='3.0'&&keys.includes('__proto__')&&Object.hasOwn(value??{},'__proto__')&&expected){assert.equal(actual,false);consumerExceptions++;}else{assert.equal(actual,expected,JSON.stringify({keys,version,input,exported}));comparisons++;}}
   }
  }
 }
}
// Data keywords and ref strings are not schema-property maps.
const data=JSON.parse('{"properties":{"__proto__":{"type":"string"},"constructor":{"type":"number"}},"default":{"properties":{"__proto__":{"sentinel":1}}},"const":{"properties":{"constructor":2}},"$ref":"#/properties/__proto__"}');
const before=JSON.stringify(data);const output=merge(data,{properties:JSON.parse('{"__proto__":{"minLength":1},"constructor":{"minimum":0}}')});
assert.equal(JSON.stringify(data),before);assert.equal(output.properties.__proto__.type,'string');assert.equal(output.properties.__proto__.minLength,1);assert.equal(output.properties.constructor.minimum,0);
assert.equal(output.$ref,'#/properties/__proto__');assert.equal(Object.prototype.sentinel,undefined);assert.equal(Object.getPrototypeOf(output.properties).sentinel,undefined);
// Existing deep merge literal-data sanitization is unchanged: no restoration.
assert.equal(Object.hasOwn(output.default.properties,'__proto__'),false);
const collision=merge({properties:{forklaunchProperty0:{type:'string'},constructor:{type:'number'}}});assert.equal(collision.properties.forklaunchProperty0.type,'string');assert.equal(collision.properties.constructor.type,'number');
const cycle={};cycle.not=cycle;assert.throws(()=>merge(cycle),/cycle/);
let deep={};for(let i=0;i<66;i++)deep={not:deep};assert.throws(()=>merge(deep),/bounds/);
let callbacks=0;const cb=()=>{callbacks++;throw Error('key callback')};
for(const key of [z.string().transform(cb),z.preprocess(cb,z.string()),z.string().default(cb)])generateSchema(z.record(key,z.number()),false,'3.1');assert.equal(callbacks,0);
for(const key of [z.string(),z.string().trim(),z.string().length(2),z.string().regex(/^a$/i),z.coerce.number()])assert.equal(generateSchema(z.record(key,z.number()),false,'3.1').propertyNames,undefined);
for(const version of ['3.0','3.1']){const override=generateSchema(z.record(z.literal('a'),z.number()).openapi({additionalProperties:true}),false,version);assert.equal(override.additionalProperties,true);}
console.log(`PASS ${comparisons} exact record-key Zod/AJV comparisons over ${exports} exports, ${consumerExceptions} explicit OAS3.0 AJV __proto__ filtering exceptions; reserved own keys/nesting/wrappers, safe merging/immutability/bounds, annotations and zero key callbacks`);

const {UniversalSdk}=load(root+'/framework/universal-sdk/src/universalSdk.ts');
const owned=await UniversalSdk.create('https://example.invalid',{});
const props={type:'object',properties:{constructor:{type:'number'},toString:{type:'number'}},additionalProperties:false};
const document={openapi:'3.1.0',info:{title:'record',version:'1'},paths:{},components:{schemas:{Target:props,Use:{$ref:'#/components/schemas/Target'}}}};
for(const schema of [props,document.components.schemas.Use]){
 const validate=owned.responseSchemas.validator(document,'/components/schemas/Use',schema);
 assert.equal(validate({}),true);assert.equal(validate({constructor:2}),true);assert.equal(validate({constructor:'bad'}),false);
}
const injected=new Ajv({strict:false,ownProperties:false});
const custom=new UniversalSdk('https://example.invalid',injected,{});
assert.equal(injected.opts.ownProperties,false);
assert.equal(custom.responseSchemas.validator(document,'/inline',props)({}),false);
assert.equal(injected.opts.ownProperties,false);
console.log('PASS default UniversalSdk own-property validation for inline/referenced schemas; injected engine options unchanged');
