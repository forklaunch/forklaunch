import assert from 'node:assert/strict';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawn} from 'node:child_process';
import http from 'node:http';
import {timingSafeEqual} from 'node:crypto';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualHttpSources(root);
const {createHmacToken}=load(path.join(root,'framework/core/src/http/createHmacToken.ts'));
const {createVoiceClient,voiceRequestProblem}=load(path.join(root,'framework/core/src/http/voiceClient.ts'));
const to='+14155550123',request={to,flow:'appointment_reminder'};
const attributes=(count,value)=>Object.fromEntries(Array.from({length:count},(_,i)=>['k'+i,value]));
const actions=[
 ...[undefined,{}, {appointmentId:'appointment-123'},attributes(20,'id'),{['A'.repeat(64)]:'value'},{note:'x'.repeat(256)},{note:'💬'.repeat(128)},attributes(15,'x'.repeat(256)),attributes(7,'💬'.repeat(128))].map(attributes=>({method:'start',request:{...request,...(attributes?{attributes}:{})},response:{callId:'call_fixture'}})),
 {method:'start',request:{to,flow:'a'.repeat(64)},response:{callId:'flow_boundary'}},
 ...[{to:'invalid'}, {flow:'arn:aws:connect:secret'}, {flow:''}, {flow:'UPPERCASE'}, {flow:'a'.repeat(65)}, {attributes:attributes(21,'id')}, {attributes:{'1key':'id'}}, {attributes:{'a-b':'id'}}, {attributes:{['a'.repeat(65)]:'id'}}, {attributes:{FL_reserved:'id'}}, {attributes:{note:'x'.repeat(257)}}, {attributes:{note:'💬'.repeat(129)}}, {attributes:attributes(16,'x'.repeat(256))}, {attributes:attributes(8,'💬'.repeat(128))}, {attributes:{note:42}}, {attributes:[]}].map(patch=>({method:'start',request:{...request,...patch},invalid:true})),
 ...['initiated','in_progress','ended'].map(status=>({method:'call',id:'call_fixture',response:{status,startedAt:'2026-10-11T01:00:00Z',...(status==='ended'?{endedAt:'2026-10-11T01:01:02Z',durationSeconds:62.5,disconnectReason:'customer'}:{})}})),
 {method:'end',id:'call_fixture',raw:''},
 {method:'end',id:'call_fixture',status:204,raw:''},
 {method:'end',id:'call_fixture',response:{}},
 ...['','../escape','id?query','a'.repeat(129)].flatMap(id=>[{method:'call',id,invalid:true},{method:'end',id,invalid:true}]),
 ...[403,404,429,503].map(status=>({method:'call',id:'call_fixture',status,retry:status===429?'2':undefined,response:{error:'private@example.test instance-secret-fixture'}})),
 {method:'call',id:'call_fixture',response:{status:'unknown',startedAt:'2026-10-11T01:00:00Z'},strict:true},
 {method:'start',request,response:{missing:'callId'},strict:true},
 {method:'end',id:'call_fixture',raw:'invalid json'},
];
let queue=[],serverError;const captured=[],nonces=new Set();
const server=http.createServer(async(request,response)=>{
 try{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);const raw=Buffer.concat(chunks).toString('utf8');
  const action=queue.shift();assert.ok(action,'unexpected request');const method=action.method==='start'?'POST':action.method==='call'?'GET':'DELETE',route='/voice/calls'+(action.method==='start'?'':'/'+action.id);assert.equal(request.method,method);assert.equal(request.url,`/mount/instance-gateway${route}`);
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(request.headers.authorization??'');assert.ok(match);
  const [,keyId,timestamp,nonce,signature]=match;assert.equal(keyId,'instance_fixture');assert.ok(!nonces.has(nonce));nonces.add(nonce);assert.ok(Math.abs(Date.now()-new Date(timestamp).getTime())<5000);
  const expected=createHmacToken({secretKey:'instance-secret-fixture',method,path:route,body:raw||undefined,timestamp:new Date(timestamp),nonce});assert.ok(timingSafeEqual(Buffer.from(signature),Buffer.from(expected)));
  captured.push({method,route,body:raw?JSON.parse(raw):null});response.writeHead(action.status??200,{'content-type':'application/json',...(action.retry?{'retry-after':action.retry}:{})});response.end(action.raw??JSON.stringify(action.response));
 }catch(error){serverError=error;response.writeHead(500);response.end('{}');}
});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','voice_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}/mount///`;
try{
 const client=createVoiceClient({gatewayUrl:base,instanceId:'instance_fixture',hmacKey:'instance-secret-fixture'}),expected=[];
 queue=actions.filter(action=>!action.invalid);
 for(const action of actions){
  try{if(action.method==='start')assert.equal(Boolean(voiceRequestProblem(action.request)),Boolean(action.invalid));const result=action.method==='start'?await client.startOutboundCall(action.request):action.method==='call'?await client.call(action.id):await client.endCall(action.id);expected.push({ok:result??null});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
 }
 if(serverError)throw serverError;assert.equal(queue.length,0);const official=captured.splice(0);queue=actions.filter(action=>!action.invalid);
 const child=spawn(path.join(metadata.target_directory,'debug/examples/voice_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 if(serverError)throw serverError;assert.equal(queue.length,0);assert.deepEqual(captured,official);
 const actual=JSON.parse(stdout);assert.equal(actual.length,actions.length);
 for(let i=0;i<actions.length;i++){
  const result=actual[i];if(actions[i].strict){assert.equal(result.error,true);continue;}
  if(expected[i].error){assert.equal(result.error,true);assert.equal(result.status,expected[i].status);assert.equal(result.retryAfterSeconds,expected[i].retryAfterSeconds);}else assert.deepEqual(result,JSON.parse(JSON.stringify(expected[i])));
 }
 for(const result of actual.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/private@example|instance-secret-fixture/);
 console.log(`PASS ${actions.length} actual TypeScript/native voice cases: flow/recipient and attribute limits, UTF-16/byte boundaries, calls/status/hang-up, empty 200/204 responses, call-ID isolation, gateway refusals, signed wire bodies, fresh nonces and retry hints`);
}finally{await new Promise(resolve=>server.close(resolve));}
