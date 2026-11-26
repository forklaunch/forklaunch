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
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('versions','silent'),{mcp:false});
for(const mode of ['none','warning','error'])app.get('/'+mode+'/:status/:body',{name:'Versions'+mode,summary:'Versions',options:{requestValidation:'error',responseValidation:mode},versions:{'z-first':{query:{match:z.enum(['both','first'])},responses:{201:z.literal('first'),203:z.literal('old')}},'a-last':{query:{match:z.enum(['both','last'])},responses:{202:z.literal('last'),203:z.literal('new')}}}},(req,res)=>{res.setHeader('x-selected',req.version);res.status(Number(req.params.status)).send(req.params.body);});
for(const auth of [false,true])app.post('/origin/'+auth,{name:'Origin'+auth,summary:'Origin',...(auth?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),versions:{only:{query:{match:z.literal('yes')},body:{flag:z.literal('ok')},responses:{200:z.unknown()}}}},(req,res)=>res.send('ORIGIN_HANDLER_RAN'));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','version_response_parity']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_response_parity'),[]);
 let count=0;
 for(const mode of ['none','warning','error'])for(const match of ['both','first','last'])for(const [status,body] of [[201,'first'],[202,'last'],[203,'old'],[203,'new'],[203,'invalid'],[204,'empty']]){
 const path='/'+mode+'/'+status+'/'+body+'?match='+match;const expectedResponse=await fetch(ts+path,{headers:{'x-correlation-id':'versions-oracle'}});const expected={status:expectedResponse.status,selected:expectedResponse.headers.get('x-selected'),body:await expectedResponse.text()};
 for(const surface of ['native','local']){const response=await fetch(rust+'/'+surface+path,{headers:{'x-correlation-id':'versions-oracle'}});const actual={status:response.status,selected:response.headers.get('x-selected'),body:await response.text()};assert.deepEqual(actual,expected,JSON.stringify({surface,mode,match,status,body}));count++;}}
 let origins=0;const diagnosticDifferences=[];
 const originCases=['match=invalid','match=%FF',''].map(query=>({path:'/error/201/first?'+query}));
 originCases.push(...[
 ['match=invalid','application/json','{"flag":"ok"}',false],
 ['match=%FF','application/json','{"flag":"ok"}',false],
 ['','application/json','{"flag":"ok"}',false],
 ['match=yes','application/json','{',false],
 ['match=yes','application/json','{"flag":"bad"}',false],
 ['match=yes','text/plain','{"flag":"ok"}',false],
 ['match=yes','application/json','{"flag":"ok"}',true]
 ].map(([query,media,body,auth])=>({path:'/origin/'+auth+'?'+query,method:'POST',media,body})));
 for(const input of originCases){
 const outcomes=[];
 for(const url of [ts,rust+'/native',rust+'/local']){const response=await fetch(url+input.path,{method:input.method??'GET',headers:{'x-correlation-id':'origin-oracle',...(input.media?{'content-type':input.media}:{})},...(input.body!==undefined?{body:input.body}:{})});outcomes.push({status:response.status,selected:response.headers.get('x-selected'),body:await response.text()});}
 for(const [index,surface] of ['native','local'].entries()){
 const actual=outcomes[index+1];assert.equal(actual.status,outcomes[0].status,JSON.stringify({surface,input,outcomes}));assert.equal(actual.selected,null);assert.ok(!actual.body.includes('CALLBACK_MUST_NOT_RUN')&&!actual.body.includes('ORIGIN_HANDLER_RAN'));
 if(actual.body!==outcomes[0].body)diagnosticDifferences.push({surface,path:input.path});origins++;}
 }
 console.log(`PASS ${origins} prehandler status/origin comparisons; ${diagnosticDifferences.length} known diagnostic-body differences explicitly retained`);
 console.log(`PASS ${count} actual TypeScript/native versioned response comparison pairs across Axum/local handlers, all modes, ordered duplicate statuses and selected versions`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
