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
const temporary=mkdtempSync(join(tmpdir(),'fl-request-fields-'));
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
import {SchemaValidator} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
let handlers=0,parsers=0,verifiers=0;
const verify=()=>{verifiers++};
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('versions','silent'),{mcp:false,json:{verify},text:{verify},urlencoded:{verify}});
for(const kind of ['json','text','form'])for(const mode of ['error','warning','none'])for(const auth of [false,true]){
 const schema=z.preprocess(value=>{parsers++;if(value==='operational'||value?.flag==='operational')throw new Error('PRIVATE_OPERATIONAL');return value},kind==='text'?z.literal('ok'):z.object({flag:z.literal('ok').optional().default('ok')}));
 const version=()=>({requestHeaders:{'x-tag':z.literal('ok')},query:{tag:z.literal('ok')},body:kind==='text'?{text:schema}:kind==='form'?{urlEncodedForm:schema}:schema,responses:{200:z.unknown()}});
 app.post('/'+kind+'/'+mode+'/'+auth,{name:kind+mode+auth,summary:'Request',options:{requestValidation:mode,responseValidation:'none'},...(auth?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),versions:{first:version(),last:version()}},(req,res)=>{handlers++;res.json({selected:req.version??null,body:req.body??null,query:req.query})});
}
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:z.unknown()}},(_req,res)=>res.json({handlers,parsers,verifiers}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 const ts=await start(process.execPath,['service.mjs']);
 let rust;
 if(process.env.FORKLAUNCH_VERSION_REQUEST_NATIVE==='1'){
  run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','version_request_diagnostics']);
  rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_request_diagnostics'),[]);
 }
 async function request(base,path,kind,tag,body){
  const before=await(await fetch(base+'/calls')).json();
  const headers={'content-type':kind==='json'?'application/json':kind==='text'?'text/plain':'application/x-www-form-urlencoded','x-correlation-id':'version-request',...(tag===null?{}:{'x-tag':tag})};
  const response=await fetch(base+path,{method:'POST',headers,body});const text=await response.text();
  const after=await(await fetch(base+'/calls')).json();
  return {status:response.status,body:response.status===200?JSON.parse(text):text,...Object.fromEntries(Object.keys(before).map(key=>[key,after[key]-before[key]]))};
 }
 let actual=0,exact=0,authStatus=0;
 async function compare(path,kind,tag,body){
  const expected=await request(ts,path,kind,tag,body);actual++;
  assert.ok(!JSON.stringify(expected).includes('PRIVATE_OPERATIONAL'));
  assert.equal(expected.handlers,expected.status===200?1:0);
  if(rust)for(const surface of ['native','local']){
   const got=await request(rust,'/'+surface+path,kind,tag,body);const label=JSON.stringify({path,kind,tag,body,expected,got});
   if(expected.status===401){assert.deepEqual({...got,body:null},{...expected,body:null},label);authStatus++;}
   else {assert.deepEqual(got,expected,label);exact++;}
  }
  return expected;
 }
 for(const kind of ['json','text','form'])for(const mode of ['error','warning','none'])for(const auth of [false,true])for(const header of ['ok','bad',null])for(const query of ['ok','bad',null])for(const flag of ['ok','bad',null]){
  const body=kind==='text'?(flag??''):kind==='json'?JSON.stringify({...flag===null?{}:{flag},drop:'x'}):new URLSearchParams({...flag===null?{}:{flag},drop:'x'}).toString();
  const path='/'+kind+'/'+mode+'/'+auth+(query===null?'':'?tag='+query);
  const result=await compare(path,kind,header,body);
  assert.equal(result.parsers,2,JSON.stringify({path,header,flag,result}));
  assert.equal(result.verifiers,1);
  const valid=header==='ok'&&query==='ok'&&(flag==='ok'||kind!=='text'&&flag===null);
  assert.equal(result.status,!valid&&mode==='error'?400:auth?401:200);
  if(result.status===200){
   assert.equal(result.body.selected,valid?'last':null);
   assert.deepEqual(result.body.body,valid?(kind==='text'?'ok':{flag:'ok'}):(kind==='text'?(flag??''):JSON.parse(kind==='json'?body:JSON.stringify(Object.fromEntries(new URLSearchParams(body))))));
   assert.deepEqual(result.body.query,query===null?{}:{tag:query});
  }
  if(result.status===400){
   assert.ok(result.body.startsWith('Version first request validation failed'));
   assert.ok(result.body.includes('Version last request validation failed'));
   const first=result.body.split('Version last request')[0];
   const paths=[...(header==='ok'?[]:['headers > x-tag']),...(query==='ok'?[]:['query > tag']),...(flag==='ok'||kind!=='text'&&flag===null?[]:[kind==='text'?'body':'body > flag'])];
   let previous=-1;
   for(const path of paths){const index=first.indexOf('Path: '+path);assert.ok(index>previous,first);previous=index;}
  }
 }
 for(const kind of ['json','text','form'])for(const mode of ['error','warning','none']){
  const body=kind==='text'?'operational':kind==='json'?'{"flag":"operational"}':'flag=operational';
  const result=await compare('/'+kind+'/'+mode+'/false?tag=bad',kind,'bad',body);
  assert.equal(result.status,500);assert.equal(result.parsers,1);assert.equal(result.verifiers,1);
 }
 for(const mode of ['error','warning','none']){
  const result=await compare('/json/'+mode+'/false?tag=ok','json','ok','{');
  assert.equal(result.status,400);assert.equal(result.parsers,0);assert.equal(result.verifiers,1);
 }
 console.log('PASS '+actual+' actual whole-request JSON/text/form cases; '+exact+' exact native/local pairs; '+authStatus+' auth-status/callback pairs (auth diagnostics outside scope)');
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
