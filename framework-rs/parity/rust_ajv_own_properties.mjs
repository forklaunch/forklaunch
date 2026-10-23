import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??'/private/tmp/fl-canonical-http-dependencies','package.json'));
const {z}=require('zod/v3'),Ajv=require('ajv');
const socketRequire=createRequire(path.join(process.env.FORKLAUNCH_TEST_SOCKET_DEPENDENCIES??'/private/tmp/fl-binary-socket-dependencies','package.json'));
let AjvDraft4;try{AjvDraft4=require('ajv-draft-04')}catch(error){if(error.code!=='MODULE_NOT_FOUND')throw error;AjvDraft4=socketRequire('ajv-draft-04')}
const {generateSchema}=load(path.join(root,'framework/validator/shims/zod-v3-openapi/zod-openapi.ts'));

const {UniversalSdk}=load(root+'/framework/universal-sdk/src/universalSdk.ts');
const {ajvSchema}=load(root+'/framework/universal-sdk/src/core/ajvSchema.ts');
const own=await UniversalSdk.create('https://example.invalid',{static:true});
let count=0;
for(const version of ['3.0','3.1'])for(const wrap of [x=>x,x=>z.array(x),x=>z.object({value:x}),x=>z.union([x,z.string()])]){
 const schema=generateSchema(wrap(z.record(z.enum(['__proto__','constructor','prototype']),z.number())),false,version);
 const document={openapi:version+'.0',info:{title:'test',version:'1'},paths:{},components:{schemas:{Target:schema,Use:{$ref:'#/components/schemas/Target'}}}};
 for(const referenced of [false,true]){
  const validate=own.responseSchemas.validator(document,'/components/schemas/Use',referenced?document.components.schemas.Use:schema);
  for(const raw of ['{}','{"__proto__":1}','{"__proto__":"bad"}','{"constructor":1}','{"prototype":"bad"}']){
   const input=JSON.parse(raw),wrapped=wrap(z.unknown());
   const value=wrapped._def.typeName==='ZodArray'?[input]:wrapped._def.typeName==='ZodObject'?{value:input}:input;
   assert.equal(validate(value),!raw.includes('bad'));count++;
  }
 }
}
const {readFileSync}=await import('node:fs');const vm=await import('node:vm');
const runtimeDirectory=root+'/framework-rs/crates/core/src/typescript/';
const runtime=['schema-runtime.ts','date-codec.ts','zod-checks.ts','zod-regex.ts'].map(name=>readFileSync(runtimeDirectory+name,'utf8').replace("import type Ajv from 'ajv';",'')).join('\n')+'\nexport {socketValidator,socketAjvSchema};';
const ts=socketRequire('typescript');const compiled=ts.transpileModule(runtime,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
const socket={};vm.runInNewContext(compiled,{socketDraft6MetaSchema:JSON.parse(readFileSync(root+'/framework-rs/crates/core/src/typescript/draft6-schema.json','utf8')),exports:socket,require:socketRequire,Buffer,URL,AbortSignal,TextDecoder,setTimeout,clearTimeout});
for(const dialect of ['http://json-schema.org/draft-04/schema#','http://json-schema.org/draft-06/schema#','http://json-schema.org/draft-07/schema#','https://json-schema.org/draft/2019-09/schema','https://json-schema.org/draft/2020-12/schema',undefined])for(const nestedId of [false,true]){
 const inner=JSON.parse('{"type":"object","properties":{"__proto__":{"type":"number"}},"patternProperties":{"^__proto__(?![\\\\s\\\\S])":{"minimum":0}},"additionalProperties":false}');
 const idKey=dialect?.includes('04')?'id':'$id';if(nestedId)inner[idKey]='https://example.invalid/inner';
 const schema={...(dialect?{$schema:dialect}:{}),type:'object',properties:{value:inner}};const before=JSON.stringify(schema);
 const validate=socket.socketValidator(schema);
 for(const [raw,expected]of [['{}',true],['{"__proto__":1}',true],['{"__proto__":-1}',false],['{"__proto__":"bad"}',false]]){assert.equal(validate({value:JSON.parse(raw)}),expected);count++;}
 assert.equal(JSON.stringify(schema),before);
}
// Existing pattern pointer targets remain unchanged; references to literal schema
// storage are followed deliberately, while unreferenced literal data is untouched.
const schema=JSON.parse('{"type":"object","properties":{"__proto__":{"type":"number"}},"additionalProperties":false,"default":{"properties":{"__proto__":{"type":"string"}}}}');
schema.patternProperties={'^__proto__(?![\\s\\S])':{minimum:0}};
const before=JSON.stringify(schema);const copy=ajvSchema(schema);
assert.equal(JSON.stringify(schema),before);assert.equal(copy.default,schema.default);
assert.equal(Object.keys(copy.patternProperties).length,2);assert.equal(copy.patternProperties['^__proto__(?![\\s\\S])'].minimum,0);
const check=new Ajv({strict:false,ownProperties:true}).compile(copy);assert.equal(check(JSON.parse('{"__proto__":-1}')),false);
const stored=JSON.parse('{"$ref":"#/default","default":{"type":"object","properties":{"__proto__":{"type":"number"}},"additionalProperties":false}}');
assert.throws(()=>ajvSchema(stored),/literal default\/const\/enum/);
for(const dialect of [undefined,'http://json-schema.org/draft-04/schema#','https://json-schema.org/draft/2020-12/schema'])assert.throws(()=>socket.socketValidator({...stored,...(dialect?{$schema:dialect}:{})}),/literal default\/const\/enum/);
// Dual-use literal storage must never change injected defaults or equality.
for(const keyword of ['default','const','enum']) {
 const hidden=JSON.parse('{"type":"object","properties":{"__proto__":{"type":"number"}},"additionalProperties":false}');
 const location=keyword==='enum'?'#/properties/config/enum/0':`#/properties/config/${keyword}`;
 const schema={type:'object',properties:{config:{[keyword]:keyword==='enum'?[hidden]:hidden},payload:{$ref:location}}};
 const before=JSON.stringify(schema);
 const injected=new Ajv({strict:false,useDefaults:true,ownProperties:true});
 const sdk=new UniversalSdk('https://example.invalid',injected,{});
 const document={openapi:'3.1.0',info:{title:'dual',version:'1'},paths:{},components:{schemas:{Target:schema,Use:{$ref:'#/components/schemas/Target'}}}};
 // Resource-local pointer needs a standalone ID when nested in OpenAPI.
 schema.$id='https://example.invalid/dual-'+keyword;
 const snapshot=JSON.stringify(document);
 assert.throws(()=>sdk.responseSchemas.validator(document,'/components/schemas/Use',document.components.schemas.Use),/literal default\/const\/enum/);
 assert.equal(JSON.stringify(document),snapshot);assert.equal(injected.opts.useDefaults,true);
 assert.throws(()=>ajvSchema(JSON.parse(before)),/literal default\/const\/enum/);
 assert.throws(()=>socket.socketValidator(JSON.parse(before)),/literal default\/const\/enum/);
}
const harmless={$ref:'#/default',default:{type:'number'}};
assert.equal(new Ajv({strict:false}).compile(ajvSchema(harmless))(2),true);

assert.equal(Object.prototype.polluted,undefined);
let deep={};for(let i=0;i<66;i++)deep={not:deep};assert.throws(()=>ajvSchema(deep),/budget/);
const cyclic={};cyclic.not=cyclic;assert.throws(()=>ajvSchema(cyclic),/budget/);
assert.throws(()=>ajvSchema({allOf:Array.from({length:10001},()=>true)}),/budget/);
const injected=new Ajv({strict:false,ownProperties:true});injected.addKeyword({keyword:'applicationOnly',validate:(expected,value)=>value===expected});
const custom=new UniversalSdk('https://example.invalid',injected,{});
const customSchema=JSON.parse('{"type":"object","properties":{"__proto__":{"applicationOnly":2}}}');
const customCheck=custom.responseSchemas.validator({openapi:'3.1.0',info:{title:'x',version:'1'},paths:{}},'/inline',customSchema);
assert.equal(customCheck(JSON.parse('{"__proto__":2}')),true);assert.equal(customCheck(JSON.parse('{"__proto__":3}')),false);assert.equal(injected.opts.ownProperties,true);
console.log(`PASS ${count} default SDK/schema and six-dialect socket own-property checks, immutable references/patterns/literal-data/budgets/custom-keyword controls`);

// Schema-bearing positions follow the engine dialect; literal storage is not
// rewritten unless a local pointer explicitly selects it as a schema.
const hidden=JSON.parse('{"properties":{"__proto__":{"type":"number"}}}');
const legacy={prefixItems:[hidden],const:hidden,default:hidden};
const legacyCopy=ajvSchema(legacy);assert.equal(legacyCopy.prefixItems,legacy.prefixItems);assert.equal(legacyCopy.const,hidden);assert.equal(legacyCopy.default,hidden);
const modern=ajvSchema({$schema:'https://json-schema.org/draft/2020-12/schema',prefixItems:[hidden]});assert.ok(modern.prefixItems[0].patternProperties);
const target=JSON.parse('{"$id":"https://example.invalid/own-target","type":"object","properties":{"__proto__":{"$id":"https://example.invalid/own-value","type":"number"}},"additionalProperties":false}');
const resourceDoc={openapi:'3.1.0',info:{title:'ids',version:'1'},paths:{},components:{schemas:{Target:target,Use:{$ref:'https://example.invalid/own-target'}}}};
const resourceBefore=JSON.stringify(resourceDoc);const resourceCheck=own.responseSchemas.validator(resourceDoc,'/components/schemas/Use',resourceDoc.components.schemas.Use);
assert.equal(resourceCheck(JSON.parse('{"__proto__":1}')),true);assert.equal(resourceCheck(JSON.parse('{"__proto__":"bad"}')),false);assert.equal(JSON.stringify(resourceDoc),resourceBefore);
assert.equal(resourceCheck(JSON.parse('{"__proto__\\n":1}')),false);
console.log('PASS dialect-inactive schema storage, standalone registered resources, $id-bearing original property targets and exact-end matching');
