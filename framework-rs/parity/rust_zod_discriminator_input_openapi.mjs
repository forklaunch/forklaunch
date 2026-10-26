import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv/dist/2020').default;
let callbacks=0;
const variants=[
 ['literal',z.literal('a')],['default',z.literal('a').default('a')],
 ['optional',z.literal('a').optional()],['nullable',z.literal('a').nullable()],
 ['nullish',z.literal('a').nullish()],['defaultOptional',z.literal('a').optional().default('a')],
 ['optionalDefault',z.literal('a').default('a').optional()],
 ['readonlyDefault',z.literal('a').default('a').readonly()],
 ['brandedDefault',z.literal('a').default('a').brand()],
 ['enumDefault',z.enum(['a','aa']).default('a')],
 ['nativeDefault',z.nativeEnum({A:'a',AA:'aa'}).default('a')],
 ['undefined',z.undefined()],['undefinedLiteral',z.literal(undefined)],
 ['nullDefault',z.null().default(null)]
];
const values=[{}, {tag:'a'},{tag:'aa'},{tag:'b'},{tag:null},{tag:1},{tag:false},{tag:'unknown'},{payload:2},{tag:'a',payload:'bad'}];
const rows=[];
let ownedCases=0;
const {UniversalSdk}=load(root+'/framework/universal-sdk/src/universalSdk.ts');
const owned=await UniversalSdk.create('https://example.invalid',{});
const engine=new Ajv({strict:false,ownProperties:true,coerceTypes:false});
const injected=new UniversalSdk('https://example.invalid',engine,{});
for(const [name,tag] of variants)for(const reverse of [false,true])for(const [wrap,wrapValue] of [
 [x=>x,x=>x],[x=>z.object({item:x}),x=>({item:x})],[x=>z.array(x),x=>[x]]
]) {
 const branches=[z.object({tag,payload:z.number().optional()}),z.object({tag:z.literal('b'),payload:z.number().optional()})];
 if(reverse)branches.reverse();
 const source=wrap(z.discriminatedUnion('tag',branches));
 const schema=validator.openapi2020(source,{pureInput:true});
 const inputs=values.map(wrapValue),expected=inputs.map(value=>validator.parse(source,value).ok);
 const ajv=new Ajv({strict:false,ownProperties:true});const validate=ajv.compile(schema);
 const document={openapi:'3.1.0',jsonSchemaDialect:schema.$schema,info:{title:'discriminator',version:'1'},paths:{},components:{schemas:{Target:schema}}};
 const consumers=[owned,injected].map(sdk=>sdk.responseSchemas.validator(document,'/components/schemas/Target',{$ref:'#/components/schemas/Target'}));
 assert.deepEqual(inputs.map(value=>validate(value)),expected,name);
 for(let i=0;i<inputs.length;i++){
  const before=JSON.stringify(inputs[i]);assert.equal(consumers[1](inputs[i]),expected[i],name);assert.equal(JSON.stringify(inputs[i]),before);
  // Owned engines retain their existing coercion policy; do not claim strict
  // source admission or immutability for null/boolean/number discriminators.
  if(values[i].tag===undefined || typeof values[i].tag==='string'){assert.equal(consumers[0](structuredClone(inputs[i])),expected[i],name);ownedCases++;}
 }
 rows.push({schema,values:inputs,ajv:{compiled:true,valid:expected}});
}
// JSON.parse objects have the canonical Object.prototype. Missing inherited
// discriminator names cannot select the registered undefined branch.
let rawPrototypeExceptions=0;
for(const key of ['__proto__','constructor','toString','hasOwnProperty'])for(const tag of [z.literal('a'),z.literal('a').optional(),z.literal('a').default('a')]) {
 const branches=[z.object(Object.fromEntries([[key,tag]])),z.object(Object.fromEntries([[key,z.literal('b')]]))];
 const source=z.discriminatedUnion(key,branches),schema=validator.openapi2020(source,{pureInput:true});
 const inputs=[{},Object.fromEntries([[key,'a']]),Object.fromEntries([[key,'b']]),Object.fromEntries([[key,'other']]),Object.fromEntries([[key,null]])].map(value=>JSON.parse(JSON.stringify(value)));
 const expected=inputs.map(value=>validator.parse(source,value).ok);
 const validate=new Ajv({strict:false,ownProperties:true}).compile(schema);
 for(let i=0;i<inputs.length;i++) {
  if(key==='__proto__' && expected[i]){assert.equal(validate(inputs[i]),false);rawPrototypeExceptions++;}
  else assert.equal(validate(inputs[i]),expected[i]);
 }
 const document={openapi:'3.1.0',jsonSchemaDialect:schema.$schema,info:{title:'prototype',version:'1'},paths:{},components:{schemas:{Target:schema}}};
 for(const sdk of [owned,injected]){
  const compiled=sdk.responseSchemas.validator(document,'/components/schemas/Target',{$ref:'#/components/schemas/Target'});
  assert.deepEqual(inputs.map(value=>compiled(structuredClone(value))),expected);if(sdk===owned)ownedCases+=inputs.length;
 }
 assert.equal(expected[0],false);
 rows.push({schema,values:inputs,ajv:{compiled:true,valid:expected}});
}
// Explicit branch required declarations preserve existing metadata authority.
for(const required of [undefined,[],['tag'],['payload']]) {
 const source=z.discriminatedUnion('tag',[z.object({tag:z.literal('a').default('a'),payload:z.number().optional()}).openapi({required}),z.object({tag:z.literal('b')})]);
 const schema=validator.openapi2020(source,{pureInput:true});
 assert.deepEqual(schema.oneOf[0].required,required);
}
const noAnnotation=z.discriminatedUnion('tag',[z.object({tag:z.literal('a').default('a')}),z.object({tag:z.literal('b')})]);
assert.equal(validator.openapi2020(noAnnotation).oneOf[0].required,undefined);
const {generateSchema}=load(root+'/framework/validator/shims/zod-v3-openapi/zod-openapi.ts');
for(const version of ['3.0','3.1'])assert.equal(generateSchema(noAnnotation,false,version).oneOf[0].required,undefined);
const overridden=validator.openapi2020(noAnnotation.openapi({oneOf:[{type:'number'}]}),{pureInput:true});
assert.equal(overridden.oneOf.some(item=>item.type==='number'),true);
const source=z.discriminatedUnion('tag',[z.object({tag:z.literal('a').default(()=>{callbacks++;throw Error('callback')})}),z.object({tag:z.literal('b')})]);
validator.openapi2020(source,{pureInput:true});assert.equal(callbacks,0);
let native=false;
if(process.env.FORKLAUNCH_DISCRIMINATOR_INPUT_NATIVE==='1') {
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','zod-checks','--example','array_cardinality_parity'],{input:rows.map(({schema,values})=>JSON.stringify({schema,values})).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:8*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(run.status,0,run.stderr);const result=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(result.length,rows.length);
 for(let i=0;i<rows.length;i++)assert.deepEqual(result[i],rows[i].ajv,JSON.stringify(rows[i].schema));native=true;
}
const count=rows.reduce((n,row)=>n+row.values.length,0);
console.log(`PASS ${count} actual Zod/${native?'native/':''}injected SDK comparisons across ${rows.length} schemas; ${count-rawPrototypeExceptions} exact raw AJV2020 +${rawPrototypeExceptions} explicit prototype-key exceptions repaired by SDK consumers; ${ownedCases} owned SDK admission controls; explicit metadata/default compatibility, reversed branch identity, callback count=${callbacks}`);
