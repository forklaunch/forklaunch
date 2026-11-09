import {z} from 'zod/v3';
import assert from 'node:assert/strict';
import {generateOpenApiSpecs,OPENAPI_DEFAULT_VERSION} from '@fixture/openapi';
import {SchemaValidator,string} from '@forklaunch/validator/zod';
import {SchemaValidator as TypeBoxValidator} from '@forklaunch/validator/typebox';

// Exercise the actual emitter with default/explicit/custom JWT headers and Basic.
for(const auth of [
 {jwt:{signatureKey:'test-only-signing-key'}},
 {jwt:{signatureKey:'test-only-signing-key'},headerName:'Authorization'},
 {jwt:{signatureKey:'test-only-signing-key'},headerName:'X-Api-Key'},
 {basic:{login:()=>true}}
]) {
 const spec=generateOpenApiSpecs(SchemaValidator(),['http://localhost'],['Fixture'],{
  basePath:'/',routes:[{basePath:'/',path:'/:id',method:'get',contractDetails:{
   name:'Get Item',summary:'Get an item',access:'protected',params:{id:string},
   auth:{...auth,allowedPermissions:new Set(['items:read'])},responses:{200:string}
  }}],routers:[],_fetchMap:{},sdk:{},sdkPaths:{}
 })[OPENAPI_DEFAULT_VERSION];
 const operation=Object.values(spec.paths)[0].get;
 assert.equal(operation.parameters[0].required,true);
 const expected='basic' in auth?'basic':auth.headerName??'Authorization';
 assert.deepEqual(operation.security,[{[expected]:[]}]);
 assert.ok(spec.components.securitySchemes[expected]);
 if(expected==='X-Api-Key')assert.deepEqual(spec.components.securitySchemes[expected],{type:'apiKey',in:'header',name:expected});
 else assert.equal(spec.components.securitySchemes[expected].scheme,expected==='basic'?'basic':'bearer');
}
console.log('PASS actual TypeScript OpenAPI required paths and resolvable default/explicit/custom JWT/Basic security schemes');

// Explicit no-body schemas must not be confused with nullable JSON bodies.
for(const validator of [SchemaValidator(),TypeBoxValidator()]) {
 for(const [schema,empty] of [[validator.undefined,true],[validator.void,true],
   [validator.null,false],[validator.nullish,false],[validator.string,false]] as const) {
  for(const response of [schema,{json:schema},{schema,contentType:'application/custom'}]) {
   const spec=generateOpenApiSpecs(validator,['http://localhost'],['Fixture'],{
    basePath:'/',routes:[{basePath:'/',path:'/empty',method:'get',contractDetails:{
     name:'Empty',summary:'Empty response',responses:{404:response},
     responseHeaders:{'x-result':validator.string}
    }}],routers:[],_fetchMap:{},sdk:{},sdkPaths:{}
   })[OPENAPI_DEFAULT_VERSION];
   const actual=Object.values(spec.paths)[0].get.responses['404'];
   assert.equal(actual.content===undefined,empty);
   assert.ok(actual.headers['x-result'],'No-body responses retain declared headers');
   if(!empty)assert.equal(Object.keys(actual.content).length,1);
  }
 }
}
console.log('PASS30actual Zod/TypeBox emitter no-body, null/nullish and typed-response cases');

// Optionality comes from each real validator's wrapper projection, including
// TypeBox defaults which are annotations rather than Zod runtime defaults.
for(const validator of [SchemaValidator(),TypeBoxValidator()]) {
 for(const [schema,required] of [[validator.string,true],[validator.optional(validator.string),false],[validator.null,true]] as const) {
  const spec=generateOpenApiSpecs(validator,['http://localhost'],['Fixture'],{
   basePath:'/',routes:[{basePath:'/',path:'/optional',method:'post',contractDetails:{
    name:'Optional',summary:'Optional body',body:{json:schema},responses:{200:validator.string}
   }}],routers:[],_fetchMap:{},sdk:{},sdkPaths:{}
  })[OPENAPI_DEFAULT_VERSION];
  assert.equal(Object.values(spec.paths)[0].post.requestBody.required,required);
 }
}
console.log('PASS6actual Zod/TypeBox required, optional and explicit-null request body metadata cases');

let rootCallbacks=0;
const forbidden=()=>{rootCallbacks++;throw new Error('Document generation must not execute root callbacks');};
for(const [schema,required] of [
 [z.string().optional(),false],
 [z.string().nullable(),true],
 [z.string().nullish(),false],
 [z.string().default(forbidden),false],
 [z.string().optional().transform(forbidden),false],
 [z.preprocess(forbidden,z.string().optional()),true],
 [z.string().optional().refine(forbidden),true],
 [z.string().default(forbidden).nullable(),false]
] as const)for(const body of [schema,{json:schema}]) {
 const spec=generateOpenApiSpecs(SchemaValidator(),['http://localhost'],['Fixture'],{
  basePath:'/',routes:[{basePath:'/',path:'/root',method:'post',contractDetails:{name:'Root',summary:'Root',body,responses:{200:z.string()}}}],routers:[],_fetchMap:{},sdk:{},sdkPaths:{}
 })[OPENAPI_DEFAULT_VERSION];
 const input=Object.values(spec.paths)[0].post.requestBody;
 assert.equal(input.required,required);
 const projected=Object.values(input.content)[0].schema;
 assert.equal(projected.default,undefined);
}
assert.equal(rootCallbacks,0);
console.log('PASS16 actual Zod bare/typed root optional/default/nullish/effects projections with zero throwing root callback execution');

const typebox=TypeBoxValidator();
const typeboxDefault=typebox.openapiRequestBody({...typebox.string,default:'annotated'});
assert.equal(typeboxDefault.required,true);
assert.equal(typeboxDefault.schema.default,'annotated');
const zodDefault=SchemaValidator().openapiRequestBody(z.string().default('runtime'));
assert.equal(zodDefault.required,false);
assert.equal(zodDefault.schema.default,undefined);
console.log('PASS TypeBox static default annotation and Zod unevaluated runtime default remain distinct');
const custom=SchemaValidator();
Object.defineProperty(custom,'openapiRequestBody',{value:undefined});
const fallback=generateOpenApiSpecs(custom,['http://localhost'],['Fixture'],{
 basePath:'/',routes:[{basePath:'/',path:'/custom',method:'post',contractDetails:{name:'Custom',summary:'Custom',body:{json:z.string().optional()},responses:{200:z.string()}}}],routers:[],_fetchMap:{},sdk:{},sdkPaths:{}
})[OPENAPI_DEFAULT_VERSION];
assert.equal(Object.values(fallback.paths)[0].post.requestBody.required,true);
console.log('PASS custom validators without the additive projection retain required-body export');
