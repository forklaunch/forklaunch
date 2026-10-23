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
const framing=[
 ['lf','data: {"id":"a"}\n\ndata: [DONE]\n\ndata: {"ignored":true}\n\n',[{id:'a'}]],
 ['crlf','data: {"text":"💬"}\r\n\r\n',[{text:'💬'}]],
 ['multiline',': comment\ndata: {"id":\ndata: "x"}\n\n',[{id:'x'}]],
 ['tail','data: {"tail":true}',[{tail:true}]],
 ['multiline-tail','data: {"id":\ndata: "x"}',[]],
 ['malformed','data: bad\n\ndata: {"ok":true}\n\n',[{ok:true}]],
 ['done-trailing-space','data: [DONE] \n\ndata: {"after":true}\n\n',[{after:true}]],
 ['bom','\ufeffdata: {"bom":true}\n\n',[{bom:true}]],
 ['later-bom','data: 1\n\n\ufeffdata: 2\n\n',[1]],
 ['primitives','data: null\n\ndata: false\n\ndata: 0\n\ndata: "text"\n\n',[null,false,0,'text']],
 ['unicode-trim','data: \u00a0{"trimmed":true}\n\ndata: \u0085{"skipped":true}\n\n',[{trimmed:true}]],
 ['mixed','data: 1\r\n\ndata: 2\n\r\n',[1,2]],
 ['cr-only','data: 1\r\rdata: 2',[]],
 ['invalid-utf8',Buffer.from([100,97,116,97,58,32,34,255,34,10,10]),['�']],
 ['incomplete-tail',Buffer.concat([Buffer.from('data: "tail"'),Buffer.from([226,130])]),['tail']],
 ['unsafe-number','data: 9007199254740993\n\n',[9007199254740992]],
 ['negative-zero','data: -0\n\n',[-0]],
];
const cases=[];
for(const [name,raw,expected] of framing)for(const fragmented of [false,true])cases.push({name,raw:Buffer.from(raw),expected,fragmented});
for(const [status,retry] of [[401,undefined],[429,'2.5'],[503,'0'],[502,'Infinity']])cases.push({name:`refusal-${status}`,raw:Buffer.from('private credential detail'),status,retry});
cases.push({name:'empty',raw:Buffer.alloc(0),expected:[]},{name:'no-content',raw:Buffer.alloc(0),status:204,expected:[]});
cases.push({name:'midstream-network',raw:Buffer.from('data: 1\n\n'),disconnect:true});
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
  if(item.expected)assert.deepEqual(expected.at(-1),{ok:item.expected},item.name);
  assert.equal(queue.length,0);if(failure)throw failure;
 }
 if(process.env.FORKLAUNCH_MODEL_STREAM_TS_ONLY==='1')console.log(`PASS ${cases.length} actual signed TypeScript SSE framing/error observations`);
 else{
  if(!process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY)execFileSync('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','instance-gateway','--example','model_gateway'],{cwd:root,stdio:'inherit',timeout:180000});
  const binary=process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY??path.join(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'),'debug/examples/model_gateway');
  const limits=(total,frame,frames=4096,millis=120000)=>({total,frame,frames,millis});
  const extras=[
   {name:'redirect-refused',raw:Buffer.from(''),status:302,location:'/redirect-probe',error:'model_gateway_request_failed'},
   {name:'many-small-frames-one-chunk',raw:Buffer.from('data: 1\n\n'.repeat(100)),limits:limits(1000,7),expected:Array(100).fill(1)},
   {name:'frame-overflow',raw:Buffer.from('data: "too big"\n\n'),limits:limits(100,7),error:'model_gateway_stream_limit'},
   {name:'frame-count',raw:Buffer.from(':comment\n\ndata: 1\n\n'),limits:limits(100,50,1),error:'model_gateway_stream_limit'},
   {name:'total-chunked',raw:Buffer.from('data: 1\n\n'),limits:limits(8,8),error:'provider_stream_limit'},
   {name:'total-length',raw:Buffer.from('data: 1\n\n'),limits:limits(8,8),length:true,error:'provider_stream_limit'},
   {name:'cancel',raw:Buffer.from('data: 1\n\n'),open:true,cancel_after:1,expected:[1]},
   {name:'drop',raw:Buffer.from('data: 1\n\n'),open:true,cancel_after:1,drop_only:true,expected:[1]},
   {name:'done-closes',raw:Buffer.from('data: [DONE]\n\n'),open:true,expected:[]},
   {name:'timeout',raw:Buffer.from('data: 1\n\n'),open:true,limits:limits(100,50,10,100),error:'provider_transport_error'},
   {name:'stream-overrides-json-timeout',raw:Buffer.from('data: 1\n\n'),delay:15200,expected:[1]},
   {name:'json-default-timeout-unchanged',kind:'models',raw:Buffer.from('{}'),delay:15200,error:'provider_transport_error'},
  ];
  async function native(items){queue=[...items];const child=spawn(binary,[],{cwd:root,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify({base,actions:items.map(item=>({kind:item.kind??'stream',body,limits:item.limits,cancel_after:item.cancel_after,drop_only:item.drop_only}))}));assert.equal(await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',resolve)}),0);if(failure)throw failure;assert.equal(queue.length,0);return JSON.parse(output);}
  const actual=await native([...cases,...extras]);
  for(let i=0;i<cases.length;i++){const value=actual[i];if(value.error){assert.doesNotMatch(value.description,/private|credential|fixture-secret/);delete value.code;delete value.description;}assert.deepEqual(value,expected[i],cases[i].name);}
  extras.forEach((item,i)=>{const value=actual[cases.length+i];if(item.error){assert.equal(value.error,true,item.name);assert.equal(value.code,item.error,item.name);assert.equal(value.status,item.status??null);}else assert.deepEqual(value,{ok:item.expected},item.name);});
  await delay(50);for(const name of ['cancel','drop','done-closes','timeout'])assert.ok(closed.has(name),`${name} releases response`);
  assert.equal(nonces.size,cases.length*2+extras.length);
  console.log(`PASS ${cases.length} actual/native signed SSE comparisons; per-frame/total/count/time bounds, early cancel/drop, HMAC fresh nonces/no retries, stream timeout override and unchanged JSON timeout`);
 }
}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
