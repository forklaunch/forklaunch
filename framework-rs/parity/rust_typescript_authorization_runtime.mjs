import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createHmac} from 'node:crypto';
import {once} from 'node:events';
import {STATUS_CODES} from 'node:http';
import {probeMcp,mcpRpc} from './rust_mcp_probe.mjs';
const textErrors=process.env.FORKLAUNCH_TEST_AUTH_TEXT==='1';
async function readBody(response){
 if(textErrors&&response.status>=400){assert.equal(response.headers.get('content-type'),'text/plain; charset=utf-8');return response.text();}
 return response.json();
}
const encode=value=>Buffer.from(JSON.stringify(value)).toString('base64url');
function token(claims){const payload=`${encode({alg:'HS256',typ:'JWT'})}.${encode({sub:'fixture-subject',exp:Math.floor(Date.now()/1000)+300,...claims})}`;return `${payload}.${createHmac('sha256','fixture-only-session-authorization-key').update(payload).digest('base64url')}`;}
const server=spawn(process.argv[2],[],{env:{...process.env,HOST:'127.0.0.1',PORT:'0',RUST_LOG:'info',MCP_ENABLED:'true',PROTOCOL:'http',VERSION:'v1',DOCS_PATH:'/docs'},stdio:['ignore','pipe','pipe']});
const exited=once(server,'exit');exited.catch(()=>{});
let errors='';server.stderr.on('data',chunk=>errors+=chunk);
const timer=setTimeout(()=>server.kill('SIGKILL'),60000);
try {
 const base=await new Promise((resolve,reject)=>{let output='';server.stdout.on('data',chunk=>{output+=chunk;const match=output.match(/listening on (http:\/\/[^\s]+)/);if(match)resolve(match[1]);});server.once('exit',()=>reject(Error(errors)));});
 await probeMcp(base,['checkPermissions','checkScope','checkFeatures','checkUnconfigured','checkBasic','checkResource','checkJwtResource','checkHmac','checkSession','checkMissingSessionCallback']);
 const cases=[
  ['permissions',{permissions:['read']},200],['permissions',{roles:['admin']},403],
  ['scope',{scopes:['team']},200],['scope',{scopes:['admin']},403],['scope',{scopes:['team','unknown']},403],
  ['features',{active_subscription:true,features:['reports']},200],['features',{features:['reports']},403],['features',{active_subscription:true},403],
  ['unconfigured',{},401]
 ];
 for(const [name,claims,status] of cases) {
  const authorization=`Bearer ${token(claims)}`;
  const response=await fetch(`${base}/policy/${name}`,{headers:{authorization}});
  assert.equal(response.status,status,`${name} ${JSON.stringify(claims)}`);
  const body=await readBody(response);
  const result=await mcpRpc(base,'tools/call',{name:`check${name[0].toUpperCase()+name.slice(1)}`,arguments:{}},{authorization});
  assert.equal(result.isError,status!==200,JSON.stringify(result));
  if(status===200){assert.deepEqual(body,{subject:'fixture-subject'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
 }
 for(const [headers,status] of [[{},401],[{authorization:'Bearer invalid'},403]]) {
  const response=await fetch(`${base}/policy/permissions`,{headers});assert.equal(response.status,status);
  const result=await mcpRpc(base,'tools/call',{name:'checkPermissions',arguments:{}},headers);assert.equal(result.isError,true);
 }
 for(const [credential,status,message] of [
  [null,401,'Authorization token required.'],
  ['basic '+Buffer.from('fixture-user:fixture-password').toString('base64'),401,'Invalid Authorization token format.'],
  ['Basic '+Buffer.from('fixture-user:wrong').toString('base64'),403,'Invalid Authorization login.'],
  ['Basic '+Buffer.from('fixture-user:').toString('base64'),401,'Invalid Authorization token format.'],
  ['Basic '+Buffer.from('fixture-user:fixture-password:discarded').toString('base64'),200,null],
 ]) {
  const headers=credential===null?{}:{authorization:credential};
  const response=await fetch(`${base}/policy/basic`,{headers});
  assert.equal(response.status,status);
  const body=await readBody(response);
  const result=await mcpRpc(base,'tools/call',{name:'checkBasic',arguments:{}},headers);
  assert.equal(result.isError,status!==200);
  if(status===200){assert.deepEqual(body,{subject:'fixture-user'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
  else {
   assert.equal(textErrors ? body : body.message, message);
   const prefix=`HTTP ${status} ${STATUS_CODES[status]}: `;
   assert.ok(result.content[0].text.startsWith(prefix));
   const mcpBody=result.content[0].text.slice(prefix.length);
   assert.equal(textErrors ? mcpBody : JSON.parse(mcpBody).message, message);
  }
 }
 // Fetch trims trailing HTTP header whitespace, so an empty token arrives as 'Basic'.
 for(const [credential,status] of [[null,401],['basic owned-opaque-token',401],['Basic ',401],['Basic unverified',403],['Basic owned-invalid-session',400],['Basic owned-reader-token',403],['Basic owned-opaque-token ignored',200]]) {
  const headers=credential===null?{}:{authorization:credential};
  const response=await fetch(`${base}/policy/resource`,{headers});
  assert.equal(response.status,status,`resource: ${credential}`);
  const body=await readBody(response);
  const result=await mcpRpc(base,'tools/call',{name:'checkResource',arguments:{}},headers);
  assert.equal(result.isError,status!==200);
  if(status===200){assert.deepEqual(body,{subject:'resource-user',team:'verified-team'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
 }
 for(const [credential,status] of [[null,401],['bearer invalid',401],['Bearer invalid',403],[`Bearer ${token({roles:['admin'],exp:1})}`,403],[`Bearer ${token({roles:['reader']})}`,403],[`Bearer ${token({roles:['admin']})} ignored`,200]]) {
  const headers=credential===null?{}:{authorization:credential};
  const response=await fetch(`${base}/policy/jwt-resource`,{headers});
  assert.equal(response.status,status,'JWT resource');
  const body=await readBody(response);
  assert.ok(!JSON.stringify(body).includes('private verification details'));
  const result=await mcpRpc(base,'tools/call',{name:'checkJwtResource',arguments:{}},headers);
  assert.equal(result.isError,status!==200);
  assert.ok(!JSON.stringify(result).includes('private verification details'));
  if(status===200){assert.deepEqual(body,{subject:'fixture-subject',team:'jwt-verified-team'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
 }
 const hmac=(key='peer',secret='fixture-hmac-secret',timestamp=new Date().toISOString())=>{
  const nonce='fixture-hmac-nonce';
  const signature=createHmac('sha256',secret).update(`GET\n/policy/hmac\nundefined${timestamp}\n${nonce}`).digest('base64');
  return `HMAC keyId=${key} ts=${timestamp} nonce=${nonce} signature=${signature}`;
 };
 for(const [credential,status] of [[null,401],['HMAC keyId=peer',403],['HMAC keyId=peer ts= nonce=n signature=s',401],[hmac('unknown'),403],[hmac('peer','wrong-secret'),403],[hmac('peer','fixture-hmac-secret','2000-01-01T00:00:00.000Z'),403],[hmac()+' ignored',200]]) {
  const headers=credential===null?{}:{authorization:credential};
  const response=await fetch(`${base}/policy/hmac`,{headers});assert.equal(response.status,status,'HMAC verifier');
  const body=await readBody(response);
  const result=await mcpRpc(base,'tools/call',{name:'checkHmac',arguments:{}},headers);assert.equal(result.isError,status!==200);
  if(status===200){assert.deepEqual(body,{subject:'trusted-peer'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
 }
 const fullSession={scopes:['team'],permissions:['read'],active_subscription:true,features:['reports']};
 for(const [name,claims,status,message] of [
  ['session',fullSession,200,null],
  ['session',{...fullSession,sub:'invalid-session'},400,'Invalid session: Session validation failed with the following errors:\n1. Path: sub\n   Message: subject rejected\n\n2. Path: root\n   Message: session rejected'],
  ['session',{...fullSession,scopes:[]},403,'Invalid scope for operation.'],
  ['session',{...fullSession,permissions:[],roles:['admin']},403,'Invalid Authorization permissions.'],
  ['session',{...fullSession,active_subscription:false},403,'Active subscription required.'],
  ['session',{...fullSession,features:[]},403,'Required features not available.'],
  ['missing-session-callback',fullSession,500,'No permission surfacing function provided.'],
 ]) {
  const headers={authorization:`Bearer ${token(claims)}`};
  const response=await fetch(`${base}/policy/${name}`,{headers});assert.equal(response.status,status,`ordered ${name}`);
  const body=await readBody(response);
  const result=await mcpRpc(base,'tools/call',{name:name==='session'?'checkSession':'checkMissingSessionCallback',arguments:{}},headers);
  assert.equal(result.isError,status!==200);
  if(status===200){assert.deepEqual(body,{subject:'fixture-subject',team:'ordered-session-team'});assert.deepEqual(JSON.parse(result.content[0].text),body);}
  else {
   assert.equal(textErrors ? body : body.message, message);
   const prefix=`HTTP ${status} ${STATUS_CODES[status]}: `;
   assert.ok(result.content[0].text.startsWith(prefix));
   const mcpBody=result.content[0].text.slice(prefix.length);
   assert.equal(textErrors ? mcpBody : JSON.parse(mcpBody).message, message);
  }
 }
 server.kill('SIGTERM');const [code,signal]=await exited;assert.equal(code,0,`${signal}: ${errors}`);
 console.log(`PASS generated verified-session authorization (${textErrors?'text':'JSON'} errors): HTTP/MCP Basic login and async Basic/JWT resource verification, HMAC signatures/freshness, permissions, exact scopes, subscriptions, features, missing policies, rejected credentials and graceful shutdown`);
} finally {clearTimeout(timer);if(server.exitCode===null&&!server.signalCode){server.kill('SIGKILL');await exited.catch(()=>{});}}
