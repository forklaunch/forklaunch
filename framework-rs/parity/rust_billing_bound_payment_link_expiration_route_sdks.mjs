// Invoked against the owned live route fixture; both clients come from its registry.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {existsSync,readFileSync,realpathSync,mkdtempSync,writeFileSync,mkdirSync,rmSync,symlinkSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHmac,randomUUID} from 'node:crypto';
import {tmpdir} from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
const [tsFile,rsFile,base,publicId,operationId]=process.argv.slice(2);
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
const sdk=exports.createForkLaunchSdk({baseUrl:base}),create=find(sdk,'expireBoundPaymentLink'),inspect=find(sdk,'inspectBoundPaymentLinkExpiration');assert(create&&inspect);
const input={operationId},route=`/bound-payment-links/${publicId}/expire`;
const created=await create({params:{publicId},body:input,headers:{authorization:sign('POST',route,JSON.stringify(input))}});assert.equal(created.status,200);assert.equal(created.response.state,'completed');assert.deepEqual(Object.keys(created.response).sort(),['operationId','publicId','state']);
const historical=await inspect({params:{publicId,operationId},headers:{authorization:sign('GET',`/bound-payment-links/${publicId}/expiration-operations/${operationId}`)}});assert.equal(historical.status,200);assert.deepEqual(historical.response,created.response);
const scratch=mkdtempSync(path.join(tmpdir(),'fl-payment-link-expiration-sdk-'));
try {
 mkdirSync(path.join(scratch,'src'));
 writeFileSync(path.join(scratch,'Cargo.toml'),'[package]\nname="bound-payment-link-expiration-sdk-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-universal-sdk={path="'+path.join(root,'framework-rs/crates/universal-sdk')+'"}\nreqwest={version="0.13",default-features=false,features=["json","query"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["rt-multi-thread","macros"]}\n');
 writeFileSync(path.join(scratch,'src/main.rs'),readFileSync(rsFile,'utf8')+`
#[tokio::main] async fn main(){
 let args:Vec<_>=std::env::args().collect();let client=BoundPaymentLinkExpirationClient::new(&args[1]);
 let headers=std::collections::BTreeMap::from([("authorization".into(),args[4].clone())]);
 let result=match args[2].as_str(){
 "create"=>{let response=client.expire_bound_payment_link_raw(ExpireBoundPaymentLinkRequest{params:ExpireBoundPaymentLinkParams{public_id:args[6].clone()},headers,body:Some(args[5].clone()),..Default::default()}).await.unwrap();let ExpireBoundPaymentLinkResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 "inspect"=>{let response=client.inspect_bound_payment_link_expiration(InspectBoundPaymentLinkExpirationRequest{params:InspectBoundPaymentLinkExpirationParams{public_id:args[6].clone(),operation_id:args[3].clone()},headers,..Default::default()}).await.unwrap();let InspectBoundPaymentLinkExpirationResponses::Status200(value)=response else{panic!("unexpected status")};serde_json::to_value(value).unwrap()},
 _=>panic!("unexpected operation")};println!("{}",result);
}`);
 const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
 run('cargo',['build','--quiet','--manifest-path',path.join(scratch,'Cargo.toml')],{env:{...process.env,CARGO_TARGET_DIR:target,CARGO_BUILD_JOBS:'1',CARGO_INCREMENTAL:'0'}});
 const binary=path.join(target,'debug/bound-payment-link-expiration-sdk-proof'),operation=operationId,raw=JSON.stringify(input);
 const created=JSON.parse(run(binary,[base,'create',operation,sign('POST',route,raw),raw,publicId]));assert.equal(created.state,'completed');assert.deepEqual(Object.keys(created).sort(),['operationId','publicId','state']);
 const historical=JSON.parse(run(binary,[base,'inspect',operation,sign('GET',`/bound-payment-links/${publicId}/expiration-operations/${operation}`),'',publicId]));assert.deepEqual(historical,created);
 console.log('PASS generated TypeScript strict typecheck +two live historical expiration/inspection calls; compiled Rust SDK +two live calls, IDs-only receipts');
} finally {rmSync(scratch,{recursive:true,force:true});}
