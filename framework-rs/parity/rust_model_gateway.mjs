import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {spawn,execFileSync} from 'node:child_process';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualHttpSources(root);
const {createModelGatewayClient}=load(path.join(root,'framework/core/src/http/modelGatewayClient.ts'));
const {createHmacToken}=load(path.join(root,'framework/core/src/http/createHmacToken.ts'));
const catalog={models:[{id:'terra',baa:true},{id:'luna',baa:false}],monthlyTokenBudget:100000,tokensUsedThisMonth:23,requestsPerMinute:7,withheldForHipaa:['other'],extra:{retained:true}};
const body={model:'terra',messages:[{role:'user',content:'Unicode 💬',extra:true}],temperature:0.2,tools:[],extra:{retained:true}};
const cases=[];
const add=(kind,name,raw,options={})=>cases.push({kind,name,raw:typeof raw==='string'||Buffer.isBuffer(raw)?raw:JSON.stringify(raw),...(kind==='chat'?{body}:{}),...options});
for(const kind of ['models','chat']){
 for(const raw of [catalog,{id:'completion',model:'terra',choices:[],extra:true},null,[],false,17,'"text"'])add(kind,'unchecked-success',raw);
 for(const raw of ['\ufeff'+JSON.stringify(catalog),'9007199254740993','{"number":9007199254740995}','private malformed JSON',''])add(kind,'decode',raw);
 add(kind,'lossy-utf8',Buffer.concat([Buffer.from('{"text":"'),Buffer.from([255]),Buffer.from('"}')]));
 add(kind,'no-content','',{status:204});
 for(const [status,retry] of [[400,undefined],[401,undefined],[403,undefined],[429,'2.5'],[503,'0'],[502,'Infinity'],[500,'-1']])add(kind,'refusal-'+status,'private upstream credential detail',{status,retry});
 for(const chunked of [false,true])add(kind,'bounded-success',JSON.stringify(catalog).padEnd(8193,' '),{chunked});
}
let queue=[],failure;const nonces=new Set();
const server=createServer(async(req,res)=>{
 try{
  const item=queue.shift();assert.ok(item,'unexpected request or automatic retry');
  const chunks=[];for await(const chunk of req)chunks.push(chunk);const bytes=Buffer.concat(chunks);
  const route=item.kind==='chat'?'/models/chat/completions':'/models';
  assert.equal(req.url,'/prefix/instance-gateway'+route);assert.equal(req.method,item.kind==='chat'?'POST':'GET');
  const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(req.headers.authorization??'');
  assert.ok(match);assert.equal(match[1],'model-fixture');assert.ok(!nonces.has(match[3]));nonces.add(match[3]);
  assert.equal(match[4],createHmacToken({method:req.method,path:route,body:bytes.length?bytes:undefined,timestamp:new Date(match[2]),nonce:match[3],secretKey:'model-only-fixture-secret'}));
  if(item.kind==='chat')assert.deepEqual(JSON.parse(bytes),item.body);else assert.equal(bytes.length,0);
  res.statusCode=item.status??200;res.setHeader('content-type','application/json');if(item.retry)res.setHeader('retry-after',item.retry);
  if(item.chunked){res.write(item.raw.slice(0,8000));setImmediate(()=>res.end(item.raw.slice(8000)));}
  else{res.setHeader('content-length',Buffer.byteLength(item.raw));res.end(item.raw);}
 }catch(error){failure=error;res.statusCode=500;res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}/prefix`;
const options={gatewayUrl:base,instanceId:'model-fixture',hmacKey:'model-only-fixture-secret'};
try{
 const client=createModelGatewayClient(options),expected=[];
 for(const item of cases){
  queue=[item];try{expected.push({ok:item.kind==='chat'?await client.chat.completions.create(item.body):await client.models()});}
  catch(error){expected.push({error:true,status:error.status??null,retryAfterSeconds:error.retryAfterSeconds??null});}
  assert.equal(queue.length,0);if(failure)throw failure;
 }
 if(process.env.FORKLAUNCH_MODEL_GATEWAY_TS_ONLY==='1')console.log(`PASS ${cases.length} actual TypeScript managed model observations`);
 else{
  if(!process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY)execFileSync('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','instance-gateway','--example','model_gateway'],{cwd:root,stdio:'inherit',timeout:180000});
  const binary=process.env.FORKLAUNCH_MODEL_GATEWAY_BINARY??path.join(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'),'debug/examples/model_gateway');
  const extras=[];for(const chunked of [false,true])for(const bytes of [1048576,1048577])extras.push({kind:chunked?'chat':'models',...(chunked?{body}:{}),name:'raw-cap-'+bytes,raw:JSON.stringify(catalog).padEnd(bytes,' '),chunked,valid:bytes===1048576});
  queue=[...cases,...extras];const child=spawn(binary,[],{cwd:root,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);
  child.stdin.end(JSON.stringify({base,actions:queue.map(({kind,body})=>({kind,body}))}));
  const exit=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',resolve);});assert.equal(exit,0);if(failure)throw failure;assert.equal(queue.length,0);
  const actual=JSON.parse(output);assert.equal(actual.length,cases.length+extras.length);
  cases.forEach((item,index)=>{const value=actual[index];if(value.error){assert.doesNotMatch(value.description,/private|credential|fixture-secret/);delete value.description;delete value.code;}assert.deepEqual(value,expected[index],item.name);});
  extras.forEach((item,index)=>{const value=actual[cases.length+index];if(item.valid)assert.deepEqual(value,{ok:catalog});else{assert.equal(value.error,true);assert.equal(value.code,'provider_invalid_response');assert.equal(value.status,null);}});
  assert.equal(nonces.size,cases.length*2+extras.length);
  const unavailable=createServer();await new Promise(resolve=>unavailable.listen(0,'127.0.0.1',resolve));const closed=`http://127.0.0.1:${unavailable.address().port}`;await new Promise(resolve=>unavailable.close(resolve));
  const child2=spawn(binary,[],{cwd:root,stdio:['pipe','pipe','inherit']});let output2='';child2.stdout.on('data',chunk=>output2+=chunk);child2.stdin.end(JSON.stringify({base:closed,actions:[{kind:'models'}]}));assert.equal(await new Promise(resolve=>child2.once('exit',resolve)),0);const failed=JSON.parse(output2)[0];assert.equal(failed.error,true);assert.equal(failed.status,null);assert.doesNotMatch(failed.description,/fixture-secret/);
  console.log(`PASS ${cases.length} actual TypeScript/native signed managed model comparisons, raw byte bounds, fresh nonces and transport failure`);
 }
}finally{await new Promise(resolve=>server.close(resolve));}
