import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,symlinkSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES (npm install the canonical-iam fixture package)');
const fixture=join(root,'framework-rs/parity/fixtures/canonical-iam');
const expected=JSON.parse(readFileSync(join(fixture,'package.json'))).dependencies;
for(const [name,version] of Object.entries(expected)) {
 assert.equal(JSON.parse(readFileSync(join(dependencies,'node_modules',name,'package.json'))).version,version,`${name} fixture version`);
}
const temporary=mkdtempSync(join(tmpdir(),'fl-request-order-'));
const children=[];
function run(command,args){
 const result=spawnSync(command,args,{cwd:temporary,env:{...process.env,CARGO_INCREMENTAL:'0'},encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});
 assert.equal(result.status,0,`${result.error??''}\n${result.stdout}\n${result.stderr}`);
}
function start(command,args){return new Promise((resolve,reject)=>{
 const child=spawn(command,args,{cwd:temporary,env:{...process.env,OTEL_SDK_DISABLED:'true',NODE_ENV:'test'},stdio:['ignore','pipe','pipe']});
 children.push(child);let output='';
 const timeout=setTimeout(()=>reject(new Error(`server readiness timeout: ${output}`)),30000);
 child.on('error',error=>{clearTimeout(timeout);reject(error);});
 child.on('exit',code=>{clearTimeout(timeout);reject(new Error(`server exited ${code}: ${output}`));});
 for(const stream of [child.stdout,child.stderr])stream.on('data',chunk=>{
  output+=chunk;const match=output.match(/READY (http:\/\/127\.0\.0\.1:\d+)/);
  if(match){clearTimeout(timeout);resolve(match[1]);}
 });
});}
try{
 symlinkSync(join(dependencies,'node_modules'),join(temporary,'node_modules'));
 for(const name of ['core','express','common','validator']){
  const dest=join(temporary,'source',name);mkdirSync(dest,{recursive:true});
  cpSync(join(root,'framework',name,'src'),join(dest,'src'),{recursive:true});
  if(name!=='core')cpSync(join(root,'framework',name,'index.ts'),join(dest,'index.ts'));
  writeFileSync(join(dest,'package.json'),JSON.stringify({name:`@forklaunch/${name}`,type:'module'}));
 }
 cpSync(join(root,'framework/validator/shims'),join(temporary,'source/validator/shims'),{recursive:true});
 writeFileSync(join(temporary,'package.json'),'{"private":true,"type":"module"}');
 writeFileSync(join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{baseUrl:'.',paths:{
  '@forklaunch/common':['source/common/index.ts'],'@forklaunch/validator':['source/validator/index.ts'],
  '@forklaunch/validator/*':['source/validator/src/*/index.ts'],'@forklaunch/core/*':['source/core/src/*/index.ts'],
  '@forklaunch/express':['source/express/index.ts']},target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true}}));
 writeFileSync(join(temporary,'service.ts'),`
import {forklaunchExpress} from '@forklaunch/express';
import {SchemaValidator,number,string} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
let handlerCalls=0;
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('validation-order','silent'),{mcp:false});
for(const mode of ['none','warning','error'])for(const auth of [false,true])app.get('/validate/'+mode+'/'+auth,{name:'Validate'+mode+auth,summary:'Validate',query:{limit:number},...(auth?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),options:{requestValidation:mode,responseValidation:'none'},responses:{200:string}},(_req,res)=>{handlerCalls++;res.send('ok');});
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:{handlerCalls:z.number()}}},(_req,res)=>res.json({handlerCalls}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','request_order_parity']);
 const ts=await start(process.execPath,['service.mjs']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/request_order_parity'),[]);
 let comparisons=0;
 for(const mode of ['none','warning','error'])for(const auth of [false,true])for(const query of ['', '?limit=bad','?limit=','?limit=2','?limit=2&limit=3']){
  const results=[];
  for(const url of [ts,rust]){
   const before=await(await fetch(url+'/calls')).json();
   const response=await fetch(url+'/validate/'+mode+'/'+auth+query,{headers:{'x-correlation-id':'order-oracle'}});
   const body=await response.text();const after=await(await fetch(url+'/calls')).json();
   results.push({status:response.status,media:response.headers.get('content-type'),body,handlerCalls:after.handlerCalls-before.handlerCalls});
  }
  assert.deepEqual(results[1],results[0],JSON.stringify({mode,auth,query,results}));comparisons++;
 }
 console.log(`PASS ${comparisons} actual TypeScript/native request validation order comparisons (${comparisons*2} observations)`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
