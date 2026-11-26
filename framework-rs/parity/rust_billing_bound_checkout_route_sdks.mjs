// Invoked against the owned live route fixture; both clients come from its registry.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {existsSync,readFileSync,realpathSync,mkdtempSync,writeFileSync,mkdirSync,rmSync,symlinkSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHmac,randomUUID} from 'node:crypto';
import {tmpdir} from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
const [tsFile,rsFile,base,customer,plan]=process.argv.slice(2);
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
const sdk=exports.createForkLaunchSdk({baseUrl:base}),create=find(sdk,'createBoundCheckoutSession'),get=find(sdk,'getBoundCreatedCheckoutSession'),inspect=find(sdk,'inspectBoundCheckoutOperation');assert(create&&get&&inspect);
const operationId=randomUUID(),body={localCustomerId:customer,localPlanId:plan,quantity:2},route=`/bound-checkout/operations/${operationId}`;
const created=await create({params:{operationId},body,headers:{authorization:sign('POST',route,JSON.stringify(body))}});assert.equal(created.status,200);assert.equal(created.response.state,'created');const publicId=created.response.publicId;assert.equal(created.response.record.customerId,customer);
const read=await get({params:{id:publicId},headers:{authorization:sign('GET',`/bound-checkout/sessions/${publicId}`)}});assert.equal(read.status,200);assert.equal(read.response.id,publicId);assert.equal(read.response.customerId,customer);
const historical=await inspect({params:{operationId},headers:{authorization:sign('GET',route)}});assert.equal(historical.status,200);assert.equal(historical.response.state,'completed');assert(!('record'in historical.response));
const scratch=mkdtempSync(path.join(tmpdir(),'fl-checkout-route-sdk-'));
try {
 mkdirSync(path.join(scratch,'src'));
 writeFileSync(path.join(scratch,'Cargo.toml'),'[package]\nname="bound-checkout-route-sdk-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-universal-sdk={path="'+path.join(root,'framework-rs/crates/universal-sdk')+'"}\nreqwest={version="0.13",default-features=false,features=["json","query"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["rt-multi-thread","macros"]}\n');
 writeFileSync(path.join(scratch,'src/main.rs'),readFileSync(rsFile,'utf8')+`
#[tokio::main] async fn main(){
 let args:Vec<_>=std::env::args().collect();let client=BoundCheckoutClient::new(&args[1]);
 let headers=std::collections::BTreeMap::from([("authorization".into(),args[4].clone())]);
 let result=match args[2].as_str(){
 "create"=>{let response=client.create_bound_checkout_session_raw(CreateBoundCheckoutSessionRequest{params:CreateBoundCheckoutSessionParams{operation_id:args[3].clone()},headers,body:Some(args[5].clone()),..Default::default()}).await.unwrap();let CreateBoundCheckoutSessionResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "get"=>{let response=client.get_bound_created_checkout_session(GetBoundCreatedCheckoutSessionRequest{params:GetBoundCreatedCheckoutSessionParams{id:args[3].clone()},headers,..Default::default()}).await.unwrap();let GetBoundCreatedCheckoutSessionResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "inspect"=>{let response=client.inspect_bound_checkout_operation(InspectBoundCheckoutOperationRequest{params:InspectBoundCheckoutOperationParams{operation_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let InspectBoundCheckoutOperationResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 _=>panic!("unexpected operation")};println!("{}",result);
}`);
 const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
 run('cargo',['build','--quiet','--manifest-path',path.join(scratch,'Cargo.toml')],{env:{...process.env,CARGO_TARGET_DIR:target,CARGO_BUILD_JOBS:'2',CARGO_INCREMENTAL:'0'}});
 const binary=path.join(target,'debug/bound-checkout-route-sdk-proof'),operation=randomUUID(),url=`/bound-checkout/operations/${operation}`,raw=JSON.stringify(body);
 const created=JSON.parse(run(binary,[base,'create',operation,sign('POST',url,raw),raw]));assert.equal(created.state,'created');assert.equal(created.record.customerId,customer);
 const read=JSON.parse(run(binary,[base,'get',created.publicId,sign('GET',`/bound-checkout/sessions/${created.publicId}`)]));assert.equal(read.id,created.publicId);assert.equal(read.customerId,customer);
 const historical=JSON.parse(run(binary,[base,'inspect',operation,sign('GET',url)]));assert.equal(historical.state,'completed');assert(!('record'in historical));
 console.log('PASS generated TypeScript strict typecheck + three live calls; compiled Rust SDK + three live calls, shared BoundCheckoutV1 contracts');
} finally {rmSync(scratch,{recursive:true,force:true});}
