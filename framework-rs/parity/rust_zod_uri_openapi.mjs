import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics,load}=actualValidator(root,undefined,{sameRealm:true});
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??'/private/tmp/fl-canonical-http-dependencies','package.json'));
const Ajv=require('ajv'),addFormats=require('ajv-formats'),{z}=require('zod/v3');
const {registerFactoryUriFormat}=load(root+'/framework/universal-sdk/src/core/uriFormat.ts');
const ajv=registerFactoryUriFormat(new Ajv({strict:false}));addFormats(ajv);
const uri=statics.uri;
const variants=[['direct',uri,x=>x],['optional',uri.optional(),x=>x],['nullable',uri.nullable(),x=>x],['array',uri.array(),x=>[x]],['nested',z.object({rows:z.array(z.object({uri:uri.optional()}))}),x=>({rows:[{uri:x}]})],['union',z.union([uri,z.literal('other')]),x=>x]];
const values=[' https://example.com ','https://example.com/a b','\0https://example.com\x1f','http://exa\tmple.com','http://a\u061db.test','http://a\u0c5cb.test','mailto:x@y','file:///x','other','','relative','https://','http://exa mple.com',null,1];
let count=0;
for(const [name,schema,wrap] of variants){const exported=validator.openapi(schema),check=ajv.compile(exported);for(const value of values){const input=wrap(value);assert.equal(check(input),validator.parse(schema,input).ok,JSON.stringify({name,input,exported}));count++;}}
assert.equal(validator.openapi(uri).format,'forklaunch-whatwg-url');assert.equal(validator.openapi(uri).pattern,undefined);
assert.equal(validator.openapi(z.string().url()).format,'uri');assert.equal(validator.openapi(uri.min(1)).format,'forklaunch-whatwg-url');assert.equal(validator.openapi(uri.email()).format,'email');
const override=validator.openapi(uri.openapi({format:'uri',pattern:'^custom$'}));assert.equal(override.format,'uri');assert.equal(override.pattern,'^custom$');
const tb=load(root+'/framework/validator/src/typebox/staticSchemaValidator.ts');assert.equal(tb.SchemaValidator().openapi(tb.uri).pattern,tb.uri.pattern);
let calls=0;const callback=()=>{calls++;throw Error('unexpected callback');};for(const schema of [uri.transform(callback),uri.refine(callback),z.preprocess(callback,uri)])validator.openapi(schema);validator.openapiRequestBody(uri.default(callback));assert.equal(calls,0);
console.log(`PASS ${count} actual factory/export/AJV comparisons and raw Zod/TypeBox/multicheck/user overrides/callback controls`);
// Exercise injected engines and independently-created referenced response engines.
const {UniversalSdk}=load(root+'/framework/universal-sdk/src/universalSdk.ts');
let factories=0;
const injected=new Ajv({strict:false});addFormats(injected);
injected.addFormat('application-custom',value=>value==='custom');
const sdk=new UniversalSdk('https://example.invalid',injected,{},undefined,()=>{factories++;return new Ajv({strict:false});});
const exported=validator.openapi(uri);
const document={openapi:'3.1.0',info:{title:'URI',version:'1'},paths:{},components:{schemas:{Target:exported,Use:{$ref:'#/components/schemas/Target'}}}};
for(const schema of [exported,document.components.schemas.Use]) {
 const check=sdk.responseSchemas.validator(document,'/components/schemas/Use',schema);
 assert.equal(check(' https://example.com/a b '),true);
 assert.equal(check('relative'),false);
}
assert.equal(factories,1);
assert.equal(injected.validate({format:'application-custom'},'custom'),true);
assert.equal(injected.validate({format:'application-custom'},'other'),false);
console.log('PASS injected UniversalSdk and isolated referenced-response AJV registration; unrelated custom format preserved');
const {readFileSync}=await import('node:fs');
const {spawnSync}=await import('node:child_process');
assert.equal(process.versions.node,'24.2.0');assert.equal(process.versions.ada,'3.2.4');
const wpt=JSON.parse(readFileSync(path.join(root,'framework-rs/parity/fixtures/rust_zod_uri_wpt.json'),'utf8'));
const corpus=new Set(wpt.inputs),punycode=require('punycode/');
for(let cp=0;cp<=0x10ffff;cp+=cp<0x10000?1:257){
 if(cp>=0xd800&&cp<=0xdfff)continue;
 const host=`a${String.fromCodePoint(cp)}b.test`;
 for(const name of [host,encodeURIComponent(host),punycode.toASCII(host)])corpus.add('http://'+name);
}
for(const value of values)if(typeof value==='string')corpus.add(value);
const list=[...corpus],check=ajv.compile(exported),expected=list.map(value=>validator.parse(uri,value).ok);
for(let i=0;i<list.length;i++)assert.equal(check(list[i]),expected[i],JSON.stringify(list[i]));
console.log(`PASS ${list.length} actual factory URI export/AJV Unicode, encoded-host and WPT admission comparisons`);
if(process.env.FORKLAUNCH_URI_SCHEMA_TS_ONLY!=='1'){
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','zod-uri','--example','uri_schema_parity'],{input:JSON.stringify(exported)+'\n'+list.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',maxBuffer:64*1024*1024,timeout:300000,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 assert.deepEqual(result.stdout.trim().split('\n').map(JSON.parse),expected);
 console.log(`PASS ${list.length} pinned Ada native schema-format comparisons`);
}
