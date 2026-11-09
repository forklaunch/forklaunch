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
const {createEmailClient,isEmailAddress,validateEmailSendRequest}=load(path.join(root,'framework/core/src/http/emailClient.ts'));
const to='pat@example.test',request={to,subject:'Appointment confirmed',text:'Sign in for details.'};
const tags=count=>Object.fromEntries(Array.from({length:count},(_,i)=>['tag_'+i,'value']));
const actions=[
 ...['pat@example.test',"o'brien+tag@example.test",'Unicode💬@example.test','a..b@x.test','a@a-b.test','a@x','@x.test','a b@x.test','a@-x.test','a@x-.test','a@x..test','a\ufeff@x.test','a\u0085@x.test','a'.repeat(247)+'@x.test','a'.repeat(248)+'@x.test'].map(address=>({method:'address',address})),
 ...[{}, {to:[to,'other@example.test'],replyTo:'reply@example.test'}, {to:Array(50).fill(to),replyTo:['reply@example.test']}, {text:'',html:'<p>Hello 💬</p>'}, {tags:{}}, {tags:tags(10)}, {tags:{'FL-custom':'VALUE_1'}}, {subject:'💬'.repeat(499)}, {text:'é'.repeat(262144)}].map(patch=>({method:'send',request:{...request,...patch},response:{messageId:'email_fixture'}})),
 ...[{to:[]}, {to:Array(51).fill(to)}, {to:'invalid'}, {replyTo:[]}, {replyTo:'invalid'}, {subject:''}, {subject:'\ufeff'}, {subject:'line\nline'}, {subject:'💬'.repeat(500)}, {text:'',html:''}, {text:'é'.repeat(262145)}, {tags:tags(11)}, {tags:{'fl-reserved':'id'}}, {tags:{'ses:reserved':'id'}}, {tags:{'':'id'}}, {tags:{name:''}}, {tags:{name:'💬'}}, {tags:{['a'.repeat(257)]:'id'}}].map(patch=>({method:'send',request:{...request,...patch},invalid:true})),
 ...[['PAT+TAG@Example.Test',true],["O'BRIEN@Example.Test",false],['Unicode💬@Example.Test',false],['pat@example.test','true']].map(([address,suppressed])=>({method:'suppressed',address,response:{suppressed}})),
 {method:'suppressed',address:'invalid',invalid:true},
 ...[422,429,503].map(status=>({method:'send',request,status,retry:status===429?'6':undefined,response:{error:'private@example.test instance-secret-fixture'}})),
 {method:'send',request,response:{missing:'messageId'},strict:true},
];
let queue=[],serverError;const captured=[],nonces=new Set();
const server=http.createServer(async(request,response)=>{
 try{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);const raw=Buffer.concat(chunks).toString('utf8');
  const action=queue.shift();assert.ok(action,'unexpected request');const method=action.method==='send'?'POST':'GET',route=action.method==='send'?'/email/send':'/email/suppressions/'+encodeURIComponent(action.address.toLowerCase());assert.equal(request.method,method);assert.equal(request.url,`/mount/instance-gateway${route}`);
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(request.headers.authorization??'');assert.ok(match);
  const [,keyId,timestamp,nonce,signature]=match;assert.equal(keyId,'instance_fixture');assert.ok(!nonces.has(nonce));nonces.add(nonce);assert.ok(Math.abs(Date.now()-new Date(timestamp).getTime())<5000);
  const expected=createHmacToken({secretKey:'instance-secret-fixture',method,path:route,body:raw||undefined,timestamp:new Date(timestamp),nonce});assert.ok(timingSafeEqual(Buffer.from(signature),Buffer.from(expected)), `signature mismatch: ${method} ${route} (${raw.length} body characters)`);
  captured.push({method,route,body:raw?JSON.parse(raw):null});response.writeHead(action.status??200,{'content-type':'application/json',...(action.retry?{'retry-after':action.retry}:{})});response.end(action.raw??JSON.stringify(action.response));
 }catch(error){serverError=error;response.writeHead(500);response.end('{}');}
});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','email_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const base=`http://127.0.0.1:${server.address().port}/mount///`;
try{
 const client=createEmailClient({gatewayUrl:base,instanceId:'instance_fixture',hmacKey:'instance-secret-fixture'}),expected=[];
 queue=actions.filter(action=>!action.invalid&&action.method!=='address');
 for(const action of actions){
  try{if(action.method==='send'&&action.invalid)assert.throws(()=>validateEmailSendRequest(action.request));const result=action.method==='address'?isEmailAddress(action.address):action.method==='send'?await client.send(action.request):await client.suppressed(action.address);expected.push({ok:result});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
 }
 if(serverError)throw serverError;assert.equal(queue.length,0);const official=captured.splice(0);queue=actions.filter(action=>!action.invalid&&action.method!=='address');
 const child=spawn(path.join(metadata.target_directory,'debug/examples/email_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 if(serverError)throw serverError;assert.equal(queue.length,0);assert.deepEqual(captured,official);
 const actual=JSON.parse(stdout);assert.equal(actual.length,actions.length);
 for(let i=0;i<actions.length;i++){
  const result=actual[i];if(actions[i].strict){assert.equal(result.error,true);continue;}
  if(expected[i].error){assert.equal(result.error,true);assert.equal(result.status,expected[i].status);assert.equal(result.retryAfterSeconds,expected[i].retryAfterSeconds);}else assert.deepEqual(result,JSON.parse(JSON.stringify(expected[i])));
 }
 for(const result of actual.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/private@example|instance-secret-fixture/);
 console.log(`PASS ${actions.length} actual TypeScript/native email cases: mailbox validation, recipient/reply arrays, subject UTF-16 and body byte limits, tag rules, suppression casing/path encoding, signed bodies, fresh nonces, gateway refusal statuses and retry hints`);
}finally{await new Promise(resolve=>server.close(resolve));}
