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
const sdk=exports.createForkLaunchSdk({baseUrl:base});
const get=find(sdk,'getBoundCreatedPlan');assert.equal(typeof get,'function');
const route=`/bound-plans/${customer}`;
const result=await get({params:{publicId:customer},headers:{authorization:sign('GET',route)}});
assert.equal(result.status,200);assert.equal(result.response.id,customer);assert.equal(result.response.stripeFields.name,'Fresh product');
const scratch=mkdtempSync(path.join(tmpdir(),'fl-plan-read-route-sdk-'));
try {
 mkdirSync(path.join(scratch,'src'));
 writeFileSync(path.join(scratch,'Cargo.toml'),'[package]\nname="bound-plan-read-route-sdk-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nforklaunch-universal-sdk={path="'+path.join(root,'framework-rs/crates/universal-sdk')+'"}\nreqwest={version="0.13",default-features=false,features=["json","query"]}\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\ntokio={version="1",features=["rt-multi-thread","macros"]}\n');
 writeFileSync(path.join(scratch,'src/main.rs'),readFileSync(rsFile,'utf8')+`
#[tokio::main] async fn main(){
 let args:Vec<_>=std::env::args().collect();let client=BoundPlanClient::new(&args[1]);
 let response=client.get_bound_created_plan(GetBoundCreatedPlanRequest{params:GetBoundCreatedPlanParams{public_id:args[2].clone()},headers:std::collections::BTreeMap::from([("authorization".into(),args[3].clone())]),..Default::default()}).await.unwrap();
 let GetBoundCreatedPlanResponses::Status200(value)=response else{panic!("unexpected status")};println!("{}",serde_json::to_value(value).unwrap());
}`);
 const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
 run('cargo',['build','--quiet','--manifest-path',path.join(scratch,'Cargo.toml')],{env:{...process.env,CARGO_TARGET_DIR:target,CARGO_BUILD_JOBS:'2',CARGO_INCREMENTAL:'0'}});
 const result=JSON.parse(run(path.join(target,'debug/bound-plan-read-route-sdk-proof'),[base,customer,sign('GET',route)]));
 assert.equal(result.id,customer);assert.equal(result.stripeFields.name,'Fresh product');
 console.log('PASS generated TypeScript strict typecheck + one live Plan read; compiled Rust SDK + one live Plan read');
} finally {rmSync(scratch,{recursive:true,force:true});}
