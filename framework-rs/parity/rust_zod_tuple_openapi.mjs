import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root);
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??'/private/tmp/fl-canonical-http-dependencies','package.json'));
const {z}=require('zod/v3');
const {generateSchema}=load(path.join(root,'framework/validator/shims/zod-v3-openapi/zod-openapi.ts'));

const Ajv2020=require('ajv/dist/2020').default;
const dialect='https://json-schema.org/draft/2020-12/schema';
const values=[null,false,0,'x',{},[],['x'],[1],[true],['x',1],['x',1,true],['x',1,'bad'],[null,1],[[],{}],['x',1,true,false]];
const items=[[],[z.string()],[z.number()],[z.string(),z.number()],[z.string().optional(),z.number().default(1)],[z.array(z.boolean()),z.object({name:z.string().optional()})]];
let comparisons=0,exports=0;
for(const entries of items)for(const rest of [undefined,z.boolean(),z.unknown()]){
 let tuple=z.tuple(entries);if(rest)tuple=tuple.rest(rest);
 for(const [shape,wrapSchema,wrap]of [['root',x=>x,x=>x],['nullable',x=>x.nullable(),x=>x],['optional',x=>x.optional(),x=>x],['object',x=>z.object({value:x}),x=>({value:x})],['array',x=>z.array(x),x=>[x]],['union',x=>z.union([x,z.string()]),x=>x]]){
  const schema=wrapSchema(tuple),exported=validator.openapi2020(schema),validate=new Ajv2020({strict:false}).compile(exported);exports++;
  assert.equal(exported.$schema,dialect);
  for(const value of values){const input=wrap(value);assert.equal(validate(input),validator.parse(schema,input).ok,JSON.stringify({shape,input,exported}));comparisons++;}
 }
}
assert.equal(JSON.stringify(validator.openapi(z.tuple([z.string()]))),'{}');
assert.equal(JSON.stringify(generateSchema(z.tuple([z.string()]),false,'3.0')),'{}');
assert.throws(()=>generateSchema(z.tuple([]),false,'3.0',{jsonSchemaDialect:dialect}),/requires OpenAPI 3.1/);
for(const schema of [z.tuple([]).openapi({$schema:'http://json-schema.org/draft-07/schema'}),z.tuple([]).default([]).openapi({$schema:'http://json-schema.org/draft-07/schema'}),z.object({value:z.tuple([]).openapi({$schema:'http://json-schema.org/draft-07/schema'})})])assert.throws(()=>validator.openapi2020(schema),/dialect conflicts/);
const nested=validator.openapi2020(z.object({value:z.tuple([z.string(),z.tuple([z.number()])])}));assert.equal(nested.properties.value.$schema,undefined);assert.equal(nested.properties.value.prefixItems[1].$schema,undefined);
const empty=validator.openapi2020(z.tuple([]));assert.equal(empty.prefixItems,undefined);assert.equal(empty.items,false);assert.equal(empty.maxItems,0);
assert.equal(validator.openapi2020(z.tuple([z.string().optional()])).minItems,1);
const overridden=validator.openapi2020(z.tuple([z.string()]).openapi({minItems:0,items:true}));assert.equal(overridden.minItems,0);assert.equal(overridden.items,true);
let callbacks=0;const callback=()=>{callbacks++;throw Error('root callback executed')};
for(const schema of [z.tuple([]).default(callback),z.tuple([]).transform(callback),z.tuple([]).refine(callback),z.preprocess(callback,z.tuple([])),z.tuple([]).default(callback).nullable()])validator.openapi2020(schema);assert.equal(callbacks,0);
let deep=z.string();for(let i=0;i<66;i++)deep=z.array(deep);assert.throws(()=>validator.openapi2020(deep),/budget/);
const tb=load(root+'/framework/validator/src/typebox/staticSchemaValidator.ts');assert.equal(tb.SchemaValidator().openapi2020,undefined);
console.log(`PASS ${comparisons} actual Zod/AJV2020 tuple comparisons over ${exports} explicit exports; baseline/OAS3.0/TypeBox preserved, slots/rest/empty/default/optional/nesting, metadata and zero root callbacks`);
