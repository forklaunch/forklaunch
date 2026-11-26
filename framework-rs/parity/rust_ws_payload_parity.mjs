import assert from 'node:assert/strict';
import {mkdtemp, readFile, writeFile, rm} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {statics:v,validator,load}=actualValidator(root);
const helpers=load(path.join(root,'framework/core/src/ws/webSocketLike.ts'));
const schemas={text:v.string,value:v.unknown,message:v.union([v.schemify({type:v.literal('chat'),text:v.string.min(1).max(32)}),v.schemify({type:v.literal('subscribe'),topic:v.string.min(1).max(32)})]),count:v.number.innerType().int().min(0).max(1000)};
const cases=[];
for(const kind of Object.keys(schemas))for(const value of ['', 'hello 水', '{broken', 'true', 0, 42, 1001, -1, 1.5, true, false, null, [], [0,false,null], {}, {type:'chat',text:'hello 水',extra:true}, {type:'subscribe',topic:'room'}, {type:'chat',text:''}, {type:'chat',text:42}, {type:'other',text:'hello'}, {type:'chat',text:'x'.repeat(33)}])cases.push({kind,action:'encode',value});
for(const kind of Object.keys(schemas))for(const text of ['', 'hello 水', '"quoted"', '{broken', 'true', 'false', '0', '42', '-1', '1.5', '1001', 'null', '[]', '[0,false,null]', '{}', '{"type":"chat","text":"hello 水","extra":true}', '{"type":"subscribe","topic":"room"}', '{"type":"chat","text":""}', '{"type":"chat","text":42}', '{"type":"other","text":"hello"}'])cases.push({kind,action:'decode',bytes:[...Buffer.from(text)]});
for(const kind of ['text','value'])cases.push({kind,action:'decode',bytes:[0xff,0,0xfe]});
for(const text of ['1e400','['.repeat(130)+'0'+']'.repeat(130)])cases.push({kind:'text',action:'decode',bytes:[...Buffer.from(text)]});
const temporary=await mkdtemp(path.join(tmpdir(),'fl-ws-payload-'));
try{
 await writeFile(path.join(temporary,'input.json'),JSON.stringify(cases));
 execFileSync('cargo',['test','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--all-features','native_ws_payload_conformance'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_WS_PAYLOAD_INPUT:path.join(temporary,'input.json'),FORKLAUNCH_TEST_WS_PAYLOAD_OUTPUT:path.join(temporary,'output.json')},encoding:'utf8',timeout:300000});
 const native=JSON.parse(await readFile(path.join(temporary,'output.json'),'utf8'));
 for(let index=0;index<cases.length;index++){
  const input=cases[index];let value,error;
  try{value=input.action==='encode'?helpers.normalizeEncodedValue(helpers.encodeSchemaValue(validator,input.value,schemas[input.kind],'fixture'),'fixture'):helpers.decodeSchemaValue(validator,Buffer.from(input.bytes),schemas[input.kind],'fixture');}catch(cause){error=cause;}
  assert.equal(native[index].ok,!error,`acceptance ${index}: ${JSON.stringify(input)} ${error??''}`);
  if(error)continue;
  if(input.action==='decode')assert.deepEqual(native[index].value,JSON.parse(JSON.stringify(value)),`decoded value ${index}`);
  else{
   const bytes=Buffer.from(native[index].bytes);
   if(typeof input.value==='object'&&input.value!==null)assert.deepEqual(JSON.parse(bytes.toString()),JSON.parse(value.toString()),`structured bytes ${index}`);
   else assert.deepEqual(bytes,value,`wire bytes ${index}`);
  }
 }
 console.log(`PASS ${cases.length} actual TypeScript/Rust WebSocket payload cases: typed unions, unknown-field stripping, text/JSON/scalars/null, validation, UTF-8 and raw wire bytes`);
}finally{await rm(temporary,{recursive:true,force:true});}
