// Execute the actual Express emitter and response parser with real Zod schemas.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { mkdtemp,writeFile,readFile,rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { File } from 'node:buffer';
import { Writable } from 'node:stream';
import { once } from 'node:events';
import { actualValidator } from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const fixture=actualValidator(root), v=fixture.statics, external=createRequire(import.meta.url);
const common={};
for(const file of ['guards/isAsyncGenerator','guards/isNever','guards/isNodeJsWriteableStream','guards/isRecord','readableStreamToAsyncIterable','safeStringify'])Object.assign(common,fixture.load(path.join(root,'framework/common/src',file+'.ts')));
const cache=new Map();
function load(file){
 if(cache.has(file))return cache.get(file);
 const exports={};cache.set(file,exports);
 const code=fixture.ts.transpileModule(readFileSync(file,'utf8'),{compilerOptions:{module:fixture.ts.ModuleKind.CommonJS,target:fixture.ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(code,{exports,File,Blob,Buffer,require(spec){
  if(spec==='@forklaunch/common')return common;
  if(spec==='@forklaunch/validator')return fixture.load(path.join(root,'framework/validator/src/shared/utils/prettyPrintParseErrors.ts'));
  if(spec==='../../telemetry/recordMetric')return {recordMetric(){throw Error('telemetry disabled in emitter fixture')}};
  if(spec.startsWith('.'))return load(path.resolve(path.dirname(file),spec+'.ts'));
  return external(spec);
 }});return exports;
}
const {enrichExpressLikeSend}=load(path.join(root,'framework/core/src/http/middleware/response/enrichExpressLikeSend.middleware.ts'));
async function emit(events, dataSchema=v.unknown){
 const chunks=[];let validations=0;
 const res=new Writable({write(chunk,_encoding,callback){chunks.push(Buffer.from(chunk));callback();}});
 res.statusCode=200;res.locals={};res.headers={};
 res.type=value=>{res.headers['content-type']=value;return res};
 res.status=value=>{res.statusCode=value;return res};
 res.getHeaders=()=>res.headers;
 res.send=value=>{res.end(value);return res};
 res.responseSchemas={responses:{200:fixture.validator.schemify({id:v.string,data:dataSchema})}};
 const req={schemaValidator:{...fixture.validator,parse(...args){validations++;return fixture.validator.parse(...args)}},contractDetails:{responses:{200:{event:{id:v.string,data:dataSchema}}}},context:{correlationId:'emitter-fixture'}};
 // The validator exposes prototype methods, so preserve its full implementation.
 req.schemaValidator=Object.create(fixture.validator);
 req.schemaValidator.parse=(...args)=>{validations++;return fixture.validator.parse(...args)};
 const finished=once(res,'finish');
 async function* source(){yield* events;}
 enrichExpressLikeSend(res,req,res,res.send,res.send,source(),false);
 await finished;
 return {bytes:Buffer.concat(chunks).toString(),status:res.statusCode,validations};
}
const temporary=await mkdtemp(path.join(tmpdir(),'fl-sse-emitter-'));
try {
 const cases=['', 'text', '水 🦀', false, 0, null, [0,null,'two'], {label:'水 🦀',active:false}].flatMap(data=>['','first'].map(id=>({id,data})));
 const input=path.join(temporary,'input.json'),output=path.join(temporary,'output.json');
 await writeFile(input,JSON.stringify(cases));
 execFileSync('cargo',['test','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--all-features','actual_typescript_emitter_fixture'],{cwd:root,env:{...process.env,CARGO_INCREMENTAL:'0',FORKLAUNCH_TEST_SSE_EMITTER_INPUT:input,FORKLAUNCH_TEST_SSE_EMITTER_OUTPUT:output},stdio:'inherit',timeout:300000});
 const native=JSON.parse(await readFile(output,'utf8'));
 const decode=bytes=>{const id=bytes.match(/^id: (.*)\n/)[1],data=bytes.match(/\ndata: (.*)\n\n$/s)[1];let parsed;try{parsed=JSON.parse(data)}catch{parsed=data}return {id,data:parsed}};
 for(let i=0;i<cases.length;i++){
  const actual=await emit([cases[i]]);assert.equal(actual.status,200);assert.equal(actual.validations,2);
  assert.deepEqual(decode(native[i]),decode(actual.bytes));
 }
 const firstOnly=await emit([{id:'one',data:'valid'},{id:'two',data:42}],v.string);
 assert.equal(firstOnly.status,200);assert.equal(firstOnly.validations,2);assert.match(firstOnly.bytes,/data: 42/);
 const invalid=await emit([{id:'one',data:42}],v.string);
 assert.equal(invalid.status,500);assert.match(invalid.bytes,/Invalid response/);
 const empty=await emit([]);assert.equal(empty.bytes,'');assert.equal(empty.validations,0);
 console.log(`PASS ${cases.length} actual TypeScript/native SSE emitter frames, first-only schema validation, first-event 500 and empty stream behavior`);
}finally{await rm(temporary,{recursive:true,force:true});}
