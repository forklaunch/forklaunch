// Invoked against the owned live route fixture; both clients come from its registry.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {existsSync,readFileSync,realpathSync,mkdtempSync,writeFileSync,mkdirSync,rmSync,symlinkSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHmac,randomUUID} from 'node:crypto';
import {tmpdir} from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
const [tsFile,rsFile,base,customer]=process.argv.slice(2);
const root=path.resolve(path.dirname(new URL(import.meta.url).pathname),'../..');
const require=createRequire(import.meta.url),compiler=realpathSync(process.env.TSC_BIN),ts=require(require.resolve('typescript',{paths:[path.dirname(compiler)]}));
// The generated client imports its schema and query runtimes; resolve them
// from the TypeScript toolchain for both the type check and the run below.
const toolchain=path.resolve(path.dirname(compiler),'../..'),clientModules=path.join(path.dirname(tsFile),'node_modules'),clientRequire=createRequire(path.join(toolchain,'route-sdk-client.cjs'));
if(!existsSync(clientModules))symlinkSync(toolchain,clientModules,'dir');
function run(command,args,options={}){const result=spawnSync(command,args,{encoding:'utf8',timeout:300000,maxBuffer:32*1024*1024,...options});assert.equal(result.status,0,result.stdout+result.stderr);return result.stdout;}
run(process.execPath,[compiler,'--strict','--noEmit','--target','ES2022','--module','NodeNext','--moduleResolution','NodeNext',tsFile]);
const exports={};vm.runInNewContext(ts.transpileModule(readFileSync(tsFile,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:clientRequire,fetch,URL,URLSearchParams,Headers,Request,Response,AbortController,AbortSignal,FormData,Blob,File,TextEncoder,TextDecoder,ReadableStream,structuredClone,setTimeout,clearTimeout,crypto,console});
function sign(method,url,body=''){const stamp=new Date().toISOString(),nonce=randomUUID(),signature=createHmac('sha256','billing-date-fixture').update(`${method}\n${url}\n${body?body+'\n':'undefined'}${stamp}\n${nonce}`).digest('base64');return `HMAC keyId=default ts=${stamp} nonce=${nonce} signature=${signature}`;}
function find(value,name){for(const[key,item]of Object.entries(value)){if(key.toLowerCase()===name.toLowerCase()&&typeof item==='function')return item.bind(value);if(item&&typeof item==='object'){const result=find(item,name);if(result)return result;}}}
const sdk=exports.createForkLaunchSdk({baseUrl:base}),create=find(sdk,'createBoundPortalSession'),get=find(sdk,'getStoredBoundPortalSession'),inspect=find(sdk,'inspectBoundPortalOperation');assert(create&&get&&inspect);
const operationId=randomUUID(),body={localCustomerId:customer},route=`/bound-portal/operations/${operationId}`;
const created=await create({params:{operationId},body,headers:{authorization:sign('POST',route,JSON.stringify(body))}});assert.equal(created.status,200);assert.equal(created.response.state,'created');const publicId=created.response.publicId;assert.equal(created.response.record.customerId,customer);
const stored=await get({params:{publicId},headers:{authorization:sign('GET',`/bound-portal/sessions/${publicId}`)}});assert.equal(stored.status,200);assert.equal(stored.response.state,'stored');assert.deepEqual(stored.response.record,created.response.record);assert(Date.parse(stored.response.observedAt)>=Date.parse(created.response.record.createdAt));
const historical=await inspect({params:{operationId},headers:{authorization:sign('GET',route)}});assert.equal(historical.status,200);assert.equal(historical.response.state,'completed');assert(!('record'in historical.response));
const expire=find(sdk,'expireStoredBoundPortalSession'),expiry=find(sdk,'inspectStoredBoundPortalExpiration');assert(expire&&expiry);
assert.equal((await expire({params:{publicId},headers:{authorization:sign('POST',`/bound-portal/sessions/${publicId}/expire`)}})).response.state,'localExpired');
assert.equal((await expiry({params:{publicId},headers:{authorization:sign('GET',`/bound-portal/sessions/${publicId}/expiration`)}})).response.state,'localExpired');
const scratch=mkdtempSync(path.join(tmpdir(),'fl-portal-route-sdk-'));
try {
 mkdirSync(path.join(scratch,'src'));
 writeFileSync(path.join(scratch,'Cargo.toml'),'[package]\nname="bound-portal-route-sdk-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-universal-sdk={path="'+path.join(root,'framework-rs/crates/universal-sdk')+'"}\nreqwest={version="0.13",default-features=false,features=["json","query"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["rt-multi-thread","macros"]}\n');
 writeFileSync(path.join(scratch,'src/main.rs'),readFileSync(rsFile,'utf8')+`
#[tokio::main] async fn main(){
 let args:Vec<_>=std::env::args().collect();let client=BoundPortalClient::new(&args[1]);
 let headers=std::collections::BTreeMap::from([("authorization".into(),args[4].clone())]);
 let result=match args[2].as_str(){
 "create"=>{let response=client.create_bound_portal_session_raw(CreateBoundPortalSessionRequest{params:CreateBoundPortalSessionParams{operation_id:args[3].clone()},headers,body:Some(args[5].clone()),..Default::default()}).await.unwrap();let CreateBoundPortalSessionResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "get"=>{let response=client.get_stored_bound_portal_session(GetStoredBoundPortalSessionRequest{params:GetStoredBoundPortalSessionParams{public_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let GetStoredBoundPortalSessionResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "inspect"=>{let response=client.inspect_bound_portal_operation(InspectBoundPortalOperationRequest{params:InspectBoundPortalOperationParams{operation_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let InspectBoundPortalOperationResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "expire"=>{let response=client.expire_stored_bound_portal_session(ExpireStoredBoundPortalSessionRequest{params:ExpireStoredBoundPortalSessionParams{public_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let ExpireStoredBoundPortalSessionResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "expiry"=>{let response=client.inspect_stored_bound_portal_expiration(InspectStoredBoundPortalExpirationRequest{params:InspectStoredBoundPortalExpirationParams{public_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let InspectStoredBoundPortalExpirationResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 _=>panic!("unexpected operation")};println!("{}",result);
}`);
 const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
 run('cargo',['build','--quiet','--manifest-path',path.join(scratch,'Cargo.toml')],{env:{...process.env,CARGO_TARGET_DIR:target,CARGO_BUILD_JOBS:'1',CARGO_INCREMENTAL:'0'}});
 const binary=path.join(target,'debug/bound-portal-route-sdk-proof'),operation=randomUUID(),url=`/bound-portal/operations/${operation}`,raw=JSON.stringify(body);
 const created=JSON.parse(run(binary,[base,'create',operation,sign('POST',url,raw),raw]));assert.equal(created.state,'created');assert.equal(created.record.customerId,customer);
 const stored=JSON.parse(run(binary,[base,'get',created.publicId,sign('GET',`/bound-portal/sessions/${created.publicId}`)]));assert.equal(stored.state,'stored');assert.deepEqual(stored.record,created.record);
 const historical=JSON.parse(run(binary,[base,'inspect',operation,sign('GET',url)]));assert.equal(historical.state,'completed');assert(!('record'in historical));
 assert.equal(JSON.parse(run(binary,[base,'expire',created.publicId,sign('POST',`/bound-portal/sessions/${created.publicId}/expire`)])).state,'localExpired');
 assert.equal(JSON.parse(run(binary,[base,'expiry',created.publicId,sign('GET',`/bound-portal/sessions/${created.publicId}/expiration`)])).state,'localExpired');
 console.log('PASS generated TypeScript strict typecheck + five live calls; compiled Rust SDK + five live calls, shared BoundPortalV1 contracts');
} finally {rmSync(scratch,{recursive:true,force:true});}
