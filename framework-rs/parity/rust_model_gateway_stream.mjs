import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {spawn,execFileSync} from 'node:child_process';
import {setTimeout as delay} from 'node:timers/promises';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualHttpSources(root);
const {createModelGatewayClient}=load(path.join(root,'framework/core/src/http/modelGatewayClient.ts'));
const {createHmacToken}=load(path.join(root,'framework/core/src/http/createHmacToken.ts'));
const body={model:'terra',messages:[{role:'user',content:'Unicode 💬'}],extra:true};
// Strict contract (shared with framework/core/src/http/modelStream.ts): only chunk
// objects with a choices array are emitted, exact terminated [DONE] is the only
// success, and anything else is a 502 with the items already yielded.
const C='"choices":[]',D='data: [DONE]\n\n';
const ok=items=>({ok:items}),fail=items=>({error:true,status:502,retryAfterSeconds:null,items});
const framing=[
 ['lf',`data: {"id":"a",${C}}\n\n${D}data: {"ignored":true}\n\n`,ok([{id:'a',choices:[]}])],
 ['crlf',`data: {"text":"💬",${C}}\r\n\r\ndata: [DONE]\r\n\r\n`,ok([{text:'💬',choices:[]}])],
 ['multiline',`: comment\ndata: {"id":\ndata: "x",${C}}\n\n${D}`,ok([{id:'x',choices:[]}])],
 ['skip-empty',`: ping\n\ndata:\n\ndata: {${C}}\n\n${D}`,ok([{choices:[]}])],
 ['missing-done',`data: {${C}}\n\n`,fail([{choices:[]}])],
 ['tail',`data: {${C}}\n\ndata: {"tail":true,${C}}`,fail([{choices:[]}])],
 ['unterminated-done',`data: {${C}}\n\ndata: [DONE]`,fail([{choices:[]}])],
 ['malformed',`data: bad\n\ndata: {${C}}\n\n${D}`,fail([])],
 ['not-a-chunk',`data: {"ok":true}\n\n${D}`,fail([])],
 ['choices-not-array',`data: {"choices":{}}\n\n${D}`,fail([])],
 ['gateway-error',`data: {${C}}\n\ndata: {${C},"error":{"message":"private"}}\n\n${D}`,fail([{choices:[]}])],
 ['falsy-error',`data: {${C},"error":null}\n\ndata: {${C},"error":0}\n\ndata: {${C},"error":""}\n\ndata: {${C},"error":false}\n\n${D}`,ok([{choices:[],error:null},{choices:[],error:0},{choices:[],error:''},{choices:[],error:false}])],
 ['done-trailing-space','data: [DONE] \n\n',fail([])],
 ['bom',`\ufeffdata: {"bom":true,${C}}\n\n${D}`,ok([{bom:true,choices:[]}])],
 ['later-bom',`data: {${C}}\n\n\ufeff${D}`,fail([{choices:[]}])],
 ['primitive',`data: {${C}}\n\ndata: null\n\n${D}`,fail([{choices:[]}])],
 ['array',`data: [1]\n\n${D}`,fail([])],
 ['unicode-trim',`data: \u00a0{"trimmed":true,${C}}\n\ndata: \u0085{${C}}\n\n${D}`,fail([{trimmed:true,choices:[]}])],
 ['mixed',`data: {"choices":[1]}\r\n\ndata: {"choices":[2]}\n\r\n${D}`,ok([{choices:[1]},{choices:[2]}])],
 ['cr-only','data: [DONE]\r\r',fail([])],
 ['invalid-utf8',Buffer.concat([Buffer.from('data: {"choices":["'),Buffer.from([255]),Buffer.from(`"]}\n\n${D}`)]),ok([{choices:['\ufffd']}])],
 ['incomplete-tail',Buffer.concat([Buffer.from(`data: {${C}}\n\n`),Buffer.from([226,130])]),fail([{choices:[]}])],
 ['unsafe-number',`data: {"choices":[9007199254740993]}\n\n${D}`,ok([{choices:[9007199254740992]}])],
 ['negative-zero',`data: {"choices":[-0]}\n\n${D}`,ok([{choices:[-0]}])],
];
const cases=[];
for(const [name,raw,expected] of framing)for(const fragmented of [false,true])cases.push({name,raw:Buffer.from(raw),expected,fragmented});
for(const [status,retry] of [[401,undefined],[429,'2.5'],[503,'0'],[502,'Infinity']])cases.push({name:`refusal-${status}`,raw:Buffer.from('private credential detail'),status,retry});
cases.push({name:'empty',raw:Buffer.alloc(0),expected:fail([])},{name:'no-content',raw:Buffer.alloc(0),status:204,expected:fail([])});
cases.push({name:'midstream-network',raw:Buffer.from(`data: {${C}}\n\n`),disconnect:true,expected:fail([{choices:[]}])});
let queue=[],failure;const nonces=new Set(),closed=new Set();
const server=createServer(async(req,res)=>{
 try{
  const item=queue.shift();assert.ok(item,'unexpected request/retry');
  const chunks=[];for await(const chunk of req)chunks.push(chunk);const bytes=Buffer.concat(chunks);
  const route=item.kind==='models'?'/models':'/models/chat/completions/stream';
  assert.equal(req.url,'/prefix/instance-gateway'+route);assert.equal(req.method,item.kind==='models'?'GET':'POST');
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(req.headers.authorization??'');
  assert.ok(match);assert.equal(match[1],'model-fixture');assert.ok(!nonces.has(match[3]));nonces.add(match[3]);
  assert.equal(match[4],createHmacToken({method:req.method,path:route,body:bytes.length?bytes:undefined,timestamp:new Date(match[2]),nonce:match[3],secretKey:'model-only-fixture-secret'}));
  if(item.kind!=='models')assert.deepEqual(JSON.parse(bytes),body);
  res.on('close',()=>closed.add(item.name));
  if(item.delay){await delay(item.delay);if(res.destroyed)return;}
  res.statusCode=item.status??200;res.setHeader('content-type','text/event-stream');if(item.retry)res.setHeader('retry-after',item.retry);
  if(item.location)res.setHeader('location',item.location);
  if(item.length)res.setHeader('content-length',item.raw.length);
  if(item.fragmented){for(const byte of item.raw){if(res.destroyed)return;res.write(Buffer.from([byte]));await delay(1);}}
  else res.write(item.raw);
  if(item.disconnect){await delay(30);res.destroy();}
  else if(!item.open)res.end();
 }catch(error){failure=error;if(!res.destroyed){res.statusCode=500;res.end();}}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}/prefix`;
const client=createModelGatewayClient({gatewayUrl:base,instanceId:'model-fixture',hmacKey:'model-only-fixture-secret'});
try{
 const expected=[];
 for(const item of cases){
  queue=[item];const items=[];
  try{for await(const value of client.chat.completions.stream(body))items.push(value);expected.push({ok:structuredClone(items)});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null,items:structuredClone(items)});}
  if(item.expected)assert.deepEqual(expected.at(-1),item.expected,item.name);
  assert.equal(queue.length,0);if(failure)throw failure;
 }
 if(process.env.FORKLAUNCH_MODEL_STREAM_TS_ONLY==='1')console.log(`PASS ${cases.length} actual signed TypeScript SSE framing/error observations`);
 else{
  if(!process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY)execFileSync('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','instance-gateway','--example','model_gateway'],{cwd:root,stdio:'inherit',timeout:180000});
  const binary=process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY??path.join(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'),'debug/examples/model_gateway');
  const limits=(total,frame,frames=4096,millis=120000)=>({total,frame,frames,millis});
  const extras=[
   {name:'redirect-refused',raw:Buffer.from(''),status:302,location:'/redirect-probe',error:'model_gateway_request_failed'},
   {name:'many-small-frames-one-chunk',raw:Buffer.from(`data: {${C}}\n\n`.repeat(100)+D),limits:limits(3000,24),expected:Array(100).fill({choices:[]})},
   {name:'frame-overflow',raw:Buffer.from('data: "too big"\n\n'),limits:limits(100,7),error:'model_gateway_stream_limit',errorStatus:502},
   {name:'frame-count',raw:Buffer.from(':comment\n\ndata: 1\n\n'),limits:limits(100,50,1),error:'model_gateway_stream_limit',errorStatus:502},
   {name:'total-chunked',raw:Buffer.from('data: 1\n\n'),limits:limits(8,8),error:'provider_stream_limit',errorStatus:502},
   {name:'total-length',raw:Buffer.from('data: 1\n\n'),limits:limits(8,8),length:true,error:'provider_stream_limit'},
   {name:'cancel',raw:Buffer.from(`data: {${C}}\n\n`),open:true,cancel_after:1,expected:[{choices:[]}]},
   {name:'drop',raw:Buffer.from(`data: {${C}}\n\n`),open:true,cancel_after:1,drop_only:true,expected:[{choices:[]}]},
   {name:'done-closes',raw:Buffer.from('data: [DONE]\n\n'),open:true,expected:[]},
   {name:'timeout',raw:Buffer.from(`data: {${C}}\n\n`),open:true,limits:limits(100,50,10,100),error:'provider_transport_error',errorStatus:502},
   {name:'stream-overrides-json-timeout',raw:Buffer.from(`data: {${C}}\n\n${D}`),delay:15200,expected:[{choices:[]}]},
   {name:'json-default-timeout-unchanged',kind:'models',raw:Buffer.from('{}'),delay:15200,error:'provider_transport_error'},
  ];
  async function native(items){queue=[...items];const child=spawn(binary,[],{cwd:root,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify({base,actions:items.map(item=>({kind:item.kind??'stream',body,limits:item.limits,cancel_after:item.cancel_after,drop_only:item.drop_only}))}));assert.equal(await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',resolve)}),0);if(failure)throw failure;assert.equal(queue.length,0);return JSON.parse(output);}
  const actual=await native([...cases,...extras]);
  for(let i=0;i<cases.length;i++){const value=actual[i];if(value.error){assert.doesNotMatch(value.description,/private|credential|fixture-secret/);delete value.code;delete value.description;}assert.deepEqual(value,expected[i],cases[i].name);}
  extras.forEach((item,i)=>{const value=actual[cases.length+i];if(item.error){assert.equal(value.error,true,item.name);assert.equal(value.code,item.error,item.name);assert.equal(value.status,item.errorStatus??item.status??null,item.name);}else assert.deepEqual(value,{ok:item.expected},item.name);});
  await delay(50);for(const name of ['cancel','drop','done-closes','timeout'])assert.ok(closed.has(name),`${name} releases response`);
  assert.equal(nonces.size,cases.length*2+extras.length);
  console.log(`PASS ${cases.length} actual/native signed SSE comparisons; per-frame/total/count/time bounds, early cancel/drop, HMAC fresh nonces/no retries, stream timeout override and unchanged JSON timeout`);
 }
}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
