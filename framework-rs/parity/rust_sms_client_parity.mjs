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
const {createSmsClient,smsSegments,isE164}=load(path.join(root,'framework/core/src/http/smsClient.ts'));
const to='+14155550123';
const bodies=['','hello','^{}\\[~]|€\f','£¥èéùìòÇØøÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉÄÖÑÜ§¿äöñüà','💬','漢字','\u0085','\ufeff',...[70,71,134,670,671].map(n=>'漢'.repeat(n)),...[160,161,306,1530,1531].map(n=>'A'.repeat(n)),...[80,81,765,766].map(n=>'^'.repeat(n)),...[35,36,335,336].map(n=>'💬'.repeat(n))];
const actions=[
 ...bodies.map(body=>({method:'segments',body})),
 ...['+1234567','+123456789012345','+0123456789','14155550123','+123456','+1234567890123456','+١٢٣٤٥٦٧'].map(to=>({method:'phone',to})),
 ...['hello','💬'.repeat(35),'漢'.repeat(670),'A'.repeat(1530),'^'.repeat(765),'\u0085'].map(body=>({method:'send',request:{to,body},response:{messageId:'sms_fixture',segments:smsSegments(body).segments}})),
 {method:'send',request:{to,body:'Promotion',purpose:'promotional'},response:{messageId:'sms_promotional',segments:1}},
 ...['',' \t\r\n','\ufeff','A'.repeat(1531),'漢'.repeat(671),'💬'.repeat(336),'^'.repeat(766)].map(body=>({method:'send',request:{to,body},invalid:true})),
 {method:'send',request:{to:'invalid',body:'hello'},invalid:true},
 {method:'send',request:{to,body:'hello',purpose:'invalid'},invalid:true},
 {method:'deferred',request:{to,body:'hello'},invalid:true},
 {method:'deferred',request:{to:'invalid',body:''},invalid:true},
 ...[422,429,503].map(status=>({method:'send',request:{to,body:'hello'},status,retry:status===429?'3':undefined,response:{error:'private@example.test instance-secret-fixture'}})),
 {method:'send',request:{to,body:'hello'},response:{messageId:'missing-segments'},strict:true},
];
let queue=[],serverError;const captured=[],nonces=new Set();
const server=http.createServer(async(request,response)=>{
 try{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);const raw=Buffer.concat(chunks).toString('utf8');
  const action=queue.shift();assert.ok(action,'unexpected request');assert.equal(request.method,'POST');assert.equal(request.url,'/mount/instance-gateway/sms/send');
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(request.headers.authorization??'');assert.ok(match);
  const [,keyId,timestamp,nonce,signature]=match;assert.equal(keyId,'instance_fixture');assert.ok(!nonces.has(nonce));nonces.add(nonce);assert.ok(Math.abs(Date.now()-new Date(timestamp).getTime())<5000);
  const expected=createHmacToken({secretKey:'instance-secret-fixture',method:'POST',path:'/sms/send',body:raw,timestamp:new Date(timestamp),nonce});assert.ok(timingSafeEqual(Buffer.from(signature),Buffer.from(expected)));
  captured.push(JSON.parse(raw));response.writeHead(action.status??200,{'content-type':'application/json',...(action.retry?{'retry-after':action.retry}:{})});response.end(JSON.stringify(action.response));
 }catch(error){serverError=error;response.writeHead(500);response.end('{}');}
});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','sms_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}/mount///`;
const managedKeys=['PLATFORM_GATEWAY_URL','INSTANCE_ID','INSTANCE_HMAC_KEY'],oldEnv=managedKeys.map(key=>process.env[key]);
for(const key of managedKeys)delete process.env[key];
try{
 const client=createSmsClient({gatewayUrl:base,instanceId:'instance_fixture',hmacKey:'instance-secret-fixture'}),deferred=createSmsClient({deferRefusal:true}),expected=[];assert.equal(client.managed,true);assert.equal(deferred.managed,false);assert.throws(()=>createSmsClient());
 queue=actions.filter(action=>action.method==='send'&&!action.invalid);
 for(const action of actions){
  try{expected.push({ok:action.method==='segments'?smsSegments(action.body):action.method==='phone'?isE164(action.to):await (action.method==='deferred'?deferred:client).send(action.request)});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
 }
 if(serverError)throw serverError;assert.equal(queue.length,0);const official=captured.splice(0);queue=actions.filter(action=>action.method==='send'&&!action.invalid);
 const child=spawn(path.join(metadata.target_directory,'debug/examples/sms_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 if(serverError)throw serverError;assert.equal(queue.length,0);assert.deepEqual(captured,official);
 const actual=JSON.parse(stdout);assert.equal(actual.length,actions.length);
 for(let i=0;i<actions.length;i++){
  const result=actual[i];if(actions[i].strict){assert.equal(result.error,true);continue;}
  if(expected[i].error){assert.equal(result.error,true);assert.equal(result.status,expected[i].status);assert.equal(result.retryAfterSeconds,expected[i].retryAfterSeconds);}else assert.deepEqual(result,JSON.parse(JSON.stringify(expected[i])));
 }
 for(const result of actual.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/private@example|instance-secret-fixture/);
 console.log(`PASS ${actions.length} actual TypeScript/native SMS cases: GSM basic/extensions, UTF-16 boundaries, segment caps, E.164, Unicode whitespace, purposes, deferred refusal, instance signatures, fresh nonces, exact wire bodies, statuses and retry hints`);
}finally{for(let i=0;i<managedKeys.length;i++){if(oldEnv[i]===undefined)delete process.env[managedKeys[i]];else process.env[managedKeys[i]]=oldEnv[i];}await new Promise(resolve=>server.close(resolve));}
