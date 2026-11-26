import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {assertJsonSnapshot} from './rust_json_snapshot.mjs';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualValidator(root);
const {generateAsyncApi}=load(path.join(root,'framework/core/src/ws/asyncApiV3Generator/asyncApiV3Generator.ts'));
const require=createRequire(import.meta.url),parserPath=require.resolve(process.env.ASYNCAPI_PARSER_PATH??'@asyncapi/parser');
const parserRequire=createRequire(parserPath),{Parser}=require(parserPath),Ajv=parserRequire('ajv');
const specValidator=new Ajv({strict:false,allErrors:true,ownProperties:true});
parserRequire('ajv-formats')(specValidator);
const validateDocument=specValidator.compile(parserRequire('@asyncapi/specs/schemas/3.0.0-without-$id.json'));
let parsed=0,parserLimitations=0;
const entry={shape:{type:'object',required:['message'],properties:{message:{type:'string'}},additionalProperties:false}};
const cases=[{path:'/chat',clientMessages:{},serverMessages:{}},{path:'/chat',clientMessages:{any:{shape:true}},serverMessages:{never:{shape:false}}}];
for(const name of ['default','room/~feed','hello 水 🦀','percent%2F','space name',"quote!'()*",'#fragment','__proto__','constructor','toString','line\nbreak']){
 const channel=name==='#fragment'?'fragment-channel':name;
 cases.push({path:'/chat',clientMessages:{request:{...entry,channel,operation:name}},serverMessages:{}});
 cases.push({path:'/chat',clientMessages:{request:{...entry,channels:[channel,'shared'],operations:['publish','ignored']}},serverMessages:{reply:{shape:{type:'string'},channel:'shared',operation:'received'}},errors:{failure:{shape:{type:'object'},channel:'errors'}}});
}
cases.push({path:'/chat',clientMessages:{request:{...entry,channel:'preferred',channels:['ignored'],operation:'preferred',operations:['ignored']}},serverMessages:{}});
cases.push({path:'/chat',clientMessages:{request:{...entry,channel:'',channels:['fallback'],operations:['chosen','ignored']}},serverMessages:{},ping:{shape:{type:'string'}},pong:{shape:{type:'string'}},closeReason:{reason:{shape:{type:'string'}}}});
cases.push({path:'/chat',clientMessages:{nested:{shape:{type:'object',properties:{any:true,never:false,list:{type:'array',items:false}},additionalProperties:false}}},serverMessages:{}});
for(const [version,dialect,shape] of [
 ['draft-04','http://json-schema.org/draft-04/schema#',{type:'number',minimum:0,exclusiveMinimum:true}],
 ['draft-06','http://json-schema.org/draft-06/schema#',{type:'string',const:'value'}],
 ['2019-09','https://json-schema.org/draft/2019-09/schema',{type:'array',items:{type:'integer'},minContains:2,contains:{minimum:0}}],
 ['2020-12','https://json-schema.org/draft/2020-12/schema',{type:'array',prefixItems:[{type:'string'}],items:false}],
])cases.push({path:'/dialects',clientMessages:{[version]:{shape:{$schema:dialect,...shape}}},serverMessages:{}});
for (const dialect of ['http://json-schema.org/draft-04/schema#', 'http://json-schema.org/draft-06/schema#']) {
 const annotations={if:42,then:false,else:true,dependentSchemas:{annotation:42},prefixItems:['literal'],unevaluatedProperties:false,unevaluatedItems:true,contentSchema:false};
 if(dialect.includes('draft-04'))Object.assign(annotations,{contains:42,propertyNames:['literal']});
 cases.push({path:'/dialect-annotations',clientMessages:{request:{shape:{$schema:dialect,type:'string',...annotations}}},serverMessages:{}});
}
const temporary=await mkdtemp(path.join(tmpdir(),'fl-ws-metadata-'));
async function parse(document){
 assert.equal(validateDocument(document),true,JSON.stringify(validateDocument.errors));
 // Parser 3.6.3 decodes percent escapes in map keys and mishandles __proto__.
 // Validate those names with the official JSON Schema and independent pointer checks.
 const encoded=JSON.stringify(document);
 if(encoded.includes('percent%2F')||Object.hasOwn(document.channels??{},'__proto__')||encoded.includes('"const":{"$ref":"literal"}')||encoded.includes('"$ref":"literal annotation"')||encoded.includes('"$ref":"literal default"')){parserLimitations++;return;}
 const result=await new Parser().parse(JSON.stringify(document));
 assert.deepEqual(result.diagnostics.filter(item=>item.severity===0),[],JSON.stringify(result.diagnostics));
 assert.ok(result.document);parsed++;
}
try{
 const input=path.join(temporary,'input.json'),output=path.join(temporary,'output.json'),references=path.join(temporary,'references.json');
 const framework=path.join(temporary,'framework.json');
 execFileSync('cargo',['test','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--all-features','framework_schema_entry_conformance'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_FRAMEWORK_SOCKET_CONTRACT:framework},stdio:'inherit',timeout:300000});
 cases.push(JSON.parse(await readFile(framework,'utf8')));
 await writeFile(input,JSON.stringify(cases));
 execFileSync('cargo',['test','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','raw::tests'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_RAW_SOCKET_INPUT:input,FORKLAUNCH_TEST_RAW_SOCKET_OUTPUT:output,FORKLAUNCH_TEST_RAW_SOCKET_REFERENCES:references},stdio:'inherit',timeout:300000});
 const native=JSON.parse(await readFile(output,'utf8'));
 const documents=[];
 for(let index=0;index<cases.length;index++){
  assert.equal(native[index].ok,true,JSON.stringify(native[index]));
  const actual=JSON.parse(JSON.stringify(generateAsyncApi(cases[index],{title:'Fixture',version:'1.0.0'})));
  assert.deepEqual(native[index].document,actual,`metadata ${index}`);
  documents.push(actual);
  for(const operation of Object.values(actual.operations??{})){
   for(const reference of [operation.channel.$ref,...operation.messages.map(message=>message.$ref)]){
    const resolved=decodeURIComponent(reference.slice(2)).split('/').reduce((value,key)=>value?.[key.replaceAll('~1','/').replaceAll('~0','~')],actual);
    assert.ok(resolved,`unresolved ${reference}`);
   }
  }
  await parse(actual);await parse(native[index].document);
 }
 await assertJsonSnapshot(root,'rust_asyncapi_metadata.snap.json',documents);
 const recursive=JSON.parse(await readFile(references,'utf8'));await parse(recursive);
 // The parser also dereferences literal $ref data inside const/examples. Keep
 // those assertions in the original output and AJV checks; parse a structural
 // copy independently to verify actual recursive schema references.
 const structural=structuredClone(recursive),payload=structural.channels['room/~ 水'].messages['publish/~'].payload;
 delete payload.const;delete payload.examples;await parse(structural);
 assert.deepEqual(recursive.channels['room/~ 水'].messages['publish/~'].payload.const,{$ref:'literal'});
 const validator=new Ajv({strict:false});validator.addSchema(recursive,'raw');
 const pointer='/channels/room~1~0 水/messages/publish~1~0/payload/properties/item';
 const item=validator.compile({$ref:'raw#'+pointer});
 assert.equal(item('hello'),true);assert.equal(item(42),false);
 const annotation=JSON.parse(await readFile(references+'.annotation.json','utf8'));await parse(annotation);
 const annotationStructural=structuredClone(annotation);
 delete annotationStructural.channels['room/~ 水'].messages['publish/~'].payload.schema.then;
 await parse(annotationStructural);
 const legacy=new Ajv({strict:false});
 legacy.addMetaSchema(parserRequire('ajv/dist/refs/json-schema-draft-06.json'));
 for(const keyword of ['if','then','else'])legacy.removeKeyword(keyword);
 legacy.addSchema(annotation,'annotation');
 const annotationPayload=legacy.compile({$ref:'annotation#/channels/room~1~0 水/messages/publish~1~0/payload/schema'});
 assert.equal(annotationPayload({item:{value:'ok'}}),true);
 assert.equal(annotationPayload({item:{value:'x'}}),false);
 assert.equal(annotationPayload({item:{value:42}}),false);
 const literal=JSON.parse(await readFile(references+'.literal.json','utf8'));await parse(literal);
 const literalStructural=structuredClone(literal);
 delete literalStructural.channels.request.messages.request.payload.default;
 delete literalStructural.channels.request.messages.request.payload.definitions.Storage;
 await parse(literalStructural);
 const literalEngine=new Ajv({strict:false});literalEngine.addSchema(literal,'literal');
 const literalPayload=literalEngine.compile({$ref:'literal#/channels/request/messages/request/payload'});
 assert.equal(literalPayload({item:'ok'}),true);
 assert.equal(literalPayload({item:'x'}),false);
 assert.equal(literalPayload({item:42}),false);
 assert.deepEqual(literal.channels.request.messages.request.payload.definitions.Storage.const,{allOf:[{$ref:'#/definitions/Value'}]});
 const dictionary=JSON.parse(await readFile(references+'.dictionary.json','utf8'));
 const originalDictionary=new Ajv({strict:false}).compile(dictionary.original);
 const dictionaryEngine=new Ajv({strict:false});dictionaryEngine.addSchema(dictionary.document,'dictionary');
 const exportedDictionary=dictionaryEngine.compile({$ref:'dictionary#/channels/request/messages/request/payload'});
 const dictionaryValues=[{item:{$ref:'#/definitions/Value'},const:'ok'},{item:{$ref:'#/definitions/Value'},const:1},{item:'wrong',const:'ok'},{item:{$ref:'#/definitions/Value',extra:1},const:'ok'},{const:'ok'},null];
 for(const value of dictionaryValues) assert.equal(exportedDictionary(value),originalDictionary(value),JSON.stringify(value));
 assert.equal(exportedDictionary(dictionaryValues[0]),true);
 assert.deepEqual(dictionaryValues.slice(1).map(value=>exportedDictionary(value)),[false,false,false,false,false]);
 console.log('PASS schema dictionary aliases retain separate schema/literal contexts: six original/exported AJV comparisons');
 console.log(`PASS ${cases.length} actual TypeScript/native raw socket metadata documents: channel fan-out, singular/plural precedence, directions, controls, escaped/special names, official AsyncAPI JSON Schema, ${parsed} parser documents (${parserLimitations} known parser limitations), recursive schema relocation and payload validation`);
}finally{await rm(temporary,{recursive:true,force:true});}
