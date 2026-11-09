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
const {createWhatsAppClient}=load(path.join(root,'framework/core/src/http/whatsappClient.ts'));
const to='+14155550123',template={to,template:'appointment_reminder',language:'en_US'};
const actions=[
 {method:'templates',response:{templates:[{name:'appointment_reminder',language:'en_US',status:'APPROVED',category:'UTILITY'},{name:'pending',language:'fr',status:'PENDING'}]}},
 {method:'templates',response:{templates:[]}},
 {method:'template',request:template,response:{messageId:'template_fixture'}},
 {method:'template',request:{...template,components:[]},response:{messageId:'empty_components'}},
 {method:'template',request:{...template,components:[{type:'body',parameters:[{type:'text',text:'Tuesday 💬 +&%'}]},{type:'button',sub_type:'url',index:'0',parameters:[{type:'text',text:'appointment-123'}]}]},response:{messageId:'components_fixture'}},
 ...['hello 💬','\u0085'].map(body=>({method:'text',request:{to,body},response:{messageId:'text_fixture'}})),
 ...['',' \r\n','\ufeff'].map(body=>({method:'text',request:{to,body},invalid:true})),
 {method:'text',request:{to:'invalid',body:'hello'},invalid:true},
 {method:'template',request:{...template,to:'invalid'},invalid:true},
 {method:'template',request:{...template,template:''},invalid:true},
 {method:'template',request:{...template,language:''},invalid:true},
 ...[403,409,422,429,503].map(status=>({method:'text',request:{to,body:'hello'},status,retry:status===429?'4':undefined,response:{error:'private@example.test instance-secret-fixture'}})),
 {method:'templates',response:{templates:[{name:'missing-fields'}]},strict:true},
 {method:'text',request:{to,body:'hello'},response:{missing:'messageId'},strict:true},
];
let queue=[],serverError;const captured=[],nonces=new Set();
const server=http.createServer(async(request,response)=>{
 try{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);const raw=Buffer.concat(chunks).toString('utf8');
  const action=queue.shift();assert.ok(action,'unexpected request');const method=action.method==='templates'?'GET':'POST',route=action.method==='templates'?'/whatsapp/templates':'/whatsapp/messages';assert.equal(request.method,method);assert.equal(request.url,`/mount/instance-gateway${route}`);
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(request.headers.authorization??'');assert.ok(match);
  const [,keyId,timestamp,nonce,signature]=match;assert.equal(keyId,'instance_fixture');assert.ok(!nonces.has(nonce));nonces.add(nonce);assert.ok(Math.abs(Date.now()-new Date(timestamp).getTime())<5000);
  const expected=createHmacToken({secretKey:'instance-secret-fixture',method,path:route,body:raw||undefined,timestamp:new Date(timestamp),nonce});assert.ok(timingSafeEqual(Buffer.from(signature),Buffer.from(expected)));
  captured.push({method,route,body:raw?JSON.parse(raw):null});response.writeHead(action.status??200,{'content-type':'application/json',...(action.retry?{'retry-after':action.retry}:{})});response.end(JSON.stringify(action.response));
 }catch(error){serverError=error;response.writeHead(500);response.end('{}');}
});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','whatsapp_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}/mount///`;
try{
 const client=createWhatsAppClient({gatewayUrl:base,instanceId:'instance_fixture',hmacKey:'instance-secret-fixture'}),expected=[];
 queue=actions.filter(action=>!action.invalid);
 for(const action of actions){
  try{expected.push({ok:action.method==='templates'?await client.templates():action.method==='template'?await client.sendTemplate(action.request):await client.sendText(action.request)});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
 }
 if(serverError)throw serverError;assert.equal(queue.length,0);const official=captured.splice(0);queue=actions.filter(action=>!action.invalid);
 const child=spawn(path.join(metadata.target_directory,'debug/examples/whatsapp_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 if(serverError)throw serverError;assert.equal(queue.length,0);assert.deepEqual(captured,official);
 const actual=JSON.parse(stdout);assert.equal(actual.length,actions.length);
 for(let i=0;i<actions.length;i++){
  const result=actual[i];if(actions[i].strict){assert.equal(result.error,true);continue;}
  if(expected[i].error){assert.equal(result.error,true);assert.equal(result.status,expected[i].status);assert.equal(result.retryAfterSeconds,expected[i].retryAfterSeconds);}else assert.deepEqual(result,JSON.parse(JSON.stringify(expected[i])));
 }
 for(const result of actual.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/private@example|instance-secret-fixture/);
 console.log(`PASS ${actions.length} actual TypeScript/native WhatsApp cases: templates/listing, component pass-through, text, E.164, Unicode whitespace, platform policy refusals, instance signatures, fresh nonces, exact wire bodies and retry hints`);
}finally{await new Promise(resolve=>server.close(resolve));}
