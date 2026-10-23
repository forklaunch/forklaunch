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
const {createPaymentsClient,PaymentsRequestError}=load(path.join(root,'framework/core/src/http/paymentsClient.ts'));
const ready={chargesEnabled:true,payoutsEnabled:true,requirementsDue:[]};
const actions=[
 {method:'status',body:ready},
 {method:'status',body:{chargesEnabled:false,payoutsEnabled:false,requirementsDue:['individual.verification.document']}},
 {method:'onboarding',returnUrl:'https://clinic.test/settings?done=1&unicode=💬',refreshUrl:'https://clinic.test/settings?retry=1',body:{url:'https://connect.stripe.test/setup/fixture'}},
 {method:'onboarding',returnUrl:'http://localhost:3000/payments',refreshUrl:'http://localhost:3000/retry',body:{url:'https://connect.stripe.test/setup/local'}},
 ...['','/relative','ftp://fixture.test'].map(returnUrl=>({method:'onboarding',returnUrl,refreshUrl:'https://clinic.test/retry',invalid:true})),
 {method:'onboarding',returnUrl:'https://clinic.test/done',refreshUrl:'/relative',invalid:true},
 {method:'status',status:429,retry:'2.5',body:{error:'private@example.test instance-secret-fixture'}},
 {method:'status',status:429,retry:'Wed, 21 Oct 2026 07:28:00 GMT',body:{error:'private@example.test'}},
 {method:'status',status:403,retry:'-5',body:{error:'private@example.test'}},
 {method:'status',status:500,retry:'Infinity',body:{error:'private@example.test'}},
 {method:'status',raw:'invalid json'},
 {method:'status',body:{chargesEnabled:'true',payoutsEnabled:true,requirementsDue:[]},strict:true},
 {method:'onboarding',returnUrl:'https://clinic.test/done',refreshUrl:'https://clinic.test/retry',body:{url:'javascript:alert(1)'},strict:true},
 {method:'status',raw:JSON.stringify({...ready,padding:'x'.repeat(1024*1024)}),strict:true},
];
const captured=[],nonces=new Set();let queue=[],serverError;
const server=http.createServer(async(request,response)=>{
 try{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);const raw=Buffer.concat(chunks).toString('utf8');
  const action=queue.shift();assert.ok(action,'unexpected request');
  const route=`/payments/${action.method==='status'?'status':'onboarding-link'}`;
  assert.equal(request.url,`/mount/instance-gateway${route}`);
  assert.equal(request.method,action.method==='status'?'GET':'POST');
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(request.headers.authorization??'');assert.ok(match);
  const [,keyId,timestamp,nonce,signature]=match;assert.equal(keyId,'instance_fixture');assert.ok(!nonces.has(nonce));nonces.add(nonce);
  assert.ok(Math.abs(Date.now()-new Date(timestamp).getTime())<5000);
  const expected=createHmacToken({secretKey:'instance-secret-fixture',method:request.method,path:route,body:raw||undefined,timestamp:new Date(timestamp),nonce});
  assert.ok(timingSafeEqual(Buffer.from(expected),Buffer.from(signature)),'actual TS signer verifies exact native JSON bytes');
  if(raw)assert.match(request.headers['content-type'],/application\/json/);
  captured.push({method:request.method,path:request.url,body:raw?JSON.parse(raw):null});
  response.writeHead(action.status??200,{'content-type':'application/json',...(action.retry?{'retry-after':action.retry}:{})});
  response.end(action.raw??JSON.stringify(action.body));
 }catch(error){serverError=error;response.writeHead(500);response.end('{}');}
});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','payments_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}/mount///`;
try{
 const client=createPaymentsClient({gatewayUrl:base,instanceId:'instance_fixture',hmacKey:'instance-secret-fixture'}),expected=[];
 queue=actions.filter(action=>!action.invalid);
 for(const action of actions){
  try{expected.push({ok:action.method==='status'?await client.status():await client.onboardingLink(action)});}
  catch(error){if(action.status)assert.ok(error instanceof PaymentsRequestError);expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
 }
 assert.equal(queue.length,0);if(serverError)throw serverError;
 const official=captured.splice(0);queue=actions.filter(action=>!action.invalid);
 const child=spawn(path.join(metadata.target_directory,'debug/examples/payments_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);
 try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 if(serverError)throw serverError;assert.equal(queue.length,0);assert.deepEqual(captured,official);
 const actual=JSON.parse(stdout);assert.equal(actual.length,actions.length);
 for(let i=0;i<actions.length;i++){
  const result=actual[i],action=actions[i];
  if(action.strict){assert.ok(result.error,'native typed response validation must reject malformed/oversized gateway payloads');continue;}
  if(expected[i].error){assert.equal(result.error,true);assert.equal(result.status,expected[i].status);assert.equal(result.retryAfterSeconds,expected[i].retryAfterSeconds);}
  else assert.deepEqual(result,expected[i]);
 }
 for(const result of actual.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/private@example|instance-secret-fixture/);
 console.log(`PASS ${actions.length} actual TypeScript/native payments cases: onboarding/status DTOs, instance HMAC, JSON wire bodies, fresh nonces, invalid redirects, refusal statuses, retry hints and sanitized errors; native additionally validates response DTOs and bounds`);
}finally{await new Promise(resolve=>server.close(resolve));}
