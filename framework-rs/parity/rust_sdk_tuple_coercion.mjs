import assert from 'node:assert/strict';
import http from 'node:http';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
import {createRequire} from 'node:module';
import path from 'node:path';
import {actualUniversalSdk} from './fixtures/rust_better_auth_frontend_client.mjs';
const {universalSdk,UniversalSdk,ResponseSchemas}=await actualUniversalSdk();
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const Ajv=require('ajv'),Ajv2020=require('ajv/dist/2020').default,addFormats=require('ajv-formats');
const {Type}=require('@sinclair/typebox');
const modern='https://json-schema.org/draft/2020-12/schema',legacy='http://json-schema.org/draft-07/schema';
const date={type:'string',format:'date-time'},binary={type:'string',format:'binary'},instant='2026-10-14T00:00:00Z';
let payload,requests=0;const server=http.createServer((_,res)=>{requests++;res.setHeader('content-type','application/json');res.end(JSON.stringify(payload));});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const host=`http://127.0.0.1:${server.address().port}`;
function document(schema,dialect,components={}){return {openapi:'3.1.0',...(dialect?{jsonSchemaDialect:dialect}:{}),info:{title:'tuple',version:'1'},components:{schemas:components},paths:{'/value':{get:{operationId:'value',responses:{200:{content:{'application/json':{schema}}}}}}}};}
const expected=[new Date(instant),Uint8Array.from([1,2,3]).buffer,new Date(instant)];
async function check(schema,dialect,value,result,components={}){const doc=document(schema,dialect,components),before=JSON.stringify(doc);payload=value;const sdk=await universalSdk({host,registryOptions:{raw:doc}});const response=await sdk.value();assert.deepEqual(response.response,result);assert.equal(JSON.stringify(doc),before);}
try {
 for(const dialect of [undefined,legacy,modern]){
  const tuple=dialect===modern?{type:'array',prefixItems:[date,binary],items:date,minItems:2}:{type:'array',items:[date,binary],additionalItems:date,minItems:2};
  await check(tuple,dialect,[instant,'AQID',instant],expected);
  await check({type:'object',properties:{rows:{type:'array',items:tuple}}},dialect,{rows:[[instant,'AQID',instant]]},{rows:[expected]});
  await check({$ref:'#/components/schemas/Tuple'},dialect,[instant,'AQID',instant],expected,{Tuple:tuple});
  const referenced={...tuple,...(dialect===modern?{prefixItems:[{$ref:'#/components/schemas/Date'},binary]}:{items:[{$ref:'#/components/schemas/Date'},binary]})};
  await check(referenced,dialect,[instant,'AQID',instant],expected,{Date:date});
 }
 await check(Type.Tuple([Type.String({format:'date-time'}),Type.String({format:'binary'})]),undefined,[instant,'AQID'],expected.slice(0,2));
 // PrefixItems has no meaning in the legacy dialect and must not coerce slots.
 await check({type:'array',prefixItems:[date,binary]},undefined,[instant,'AQID'],[instant,'AQID']);
 await check({type:'array',prefixItems:[{anyOf:[date,{type:'null'}]},binary],items:false},modern,[null,'AQID'],[null,expected[1]]);
 await check({type:'array',prefixItems:[{type:'string',format:'forklaunch-whatwg-url'}],items:false},modern,[' https://example.com/a b '],[' https://example.com/a b ']);
 await check(Type.Object({dates:Type.Array(Type.String({format:'date-time'}))}),undefined,{dates:[instant]},{dates:[new Date(instant)]});
 const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
 const {validator}=actualValidator(root),{z}=require('zod/v3');
 const emitted=validator.openapi2020(z.tuple([validator.date,validator.binary]).rest(validator.date));
 await check(emitted,undefined,[instant,'AQID',instant],expected);
 await check(validator.openapi2020(z.object({rows:z.array(z.tuple([validator.date,validator.binary]))})),undefined,{rows:[[instant,'AQID']]},{rows:[expected.slice(0,2)]});
 await check(validator.openapi2020(z.tuple([])),undefined,[],[]);
 const schema={type:'array',prefixItems:[date],items:false,minItems:1};
 const doc=document(schema,modern);const direct=new ResponseSchemas(new Ajv({strict:false,coerceTypes:true}));
 const input=[instant];assert.throws(()=>direct.validator(doc,'/unused',schema),/does not support selected dialect/);assert.throws(()=>direct.coerce(doc,'/unused',schema,input),/does not support selected dialect/);assert.deepEqual(input,[instant]);
 const conflict=document({...schema,$schema:legacy},modern);assert.throws(()=>direct.validator(conflict,'/unused',conflict.paths['/value'].get.responses[200].content['application/json'].schema),/conflicts/);
 const contexts=[];const source=new Ajv({strict:false});
 const custom=new UniversalSdk(host,source,{},undefined,context=>{assert.equal(Object.isFrozen(context),true);contexts.push({...context});const engine=new Ajv2020({strict:false,ownProperties:true,coerceTypes:false});addFormats(engine);engine.addKeyword({keyword:'applicationOnly',validate:(expected,value)=>expected===value});return engine;});
 const special={type:'array',prefixItems:[{type:'string',applicationOnly:'yes'}],items:false};const specialDoc=document(special,modern);
 for(let i=0;i<3;i++){const validate=custom.responseSchemas.validator(specialDoc,'/unused',special);assert.equal(validate(['yes']),true);assert.equal(validate(['no']),false);}
 assert.deepEqual(contexts,[{dialect:modern,explicit:true}]);assert.equal(source.opts.coerceTypes,undefined);
 const mismatched=new UniversalSdk(host,source,{},undefined,()=>new Ajv({strict:false}));assert.throws(()=>mismatched.responseSchemas.validator(specialDoc,'/unused',special),/does not support/);
 const inlineMixed=document(special,modern,{Legacy:{$schema:legacy,type:'string'}});assert.throws(()=>custom.responseSchemas.validator(inlineMixed,'/unused',special),/Mixed/);
 const mixed=document({$ref:'#/components/schemas/Tuple'},modern,{Tuple:{$schema:legacy,type:'array',items:[date]}});assert.throws(()=>custom.responseSchemas.validator(mixed,'/paths/~1value/get/responses/200/content/application~1json/schema',mixed.paths['/value'].get.responses[200].content['application/json'].schema),/Mixed/);
 const owned=await UniversalSdk.create(host,{});const unknown=document(schema,'https://example.invalid/dialect');assert.throws(()=>owned.responseSchemas.validator(unknown,'/unused',schema),/Unsupported owned/);
 const legacyResponses=new ResponseSchemas(new Ajv({strict:false}));const many=Array.from({length:10001},()=>instant);assert.throws(()=>legacyResponses.coerce(document({type:'array',items:[date],additionalItems:date}),'/unused',{type:'array',items:[date],additionalItems:date},many),/budget/);assert.equal(many[0],instant);
 console.log(`PASS ${requests} real HTTP tuple response conversions, actual TypeBox legacy tuple, modern/default dialect selection, references, custom factory reuse, conflicting/unsupported engine rejection and immutable budgets`);
} finally {await new Promise(resolve=>server.close(resolve));}
