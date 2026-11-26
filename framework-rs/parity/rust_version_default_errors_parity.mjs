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
 children.push(child);child.stderr.on('data',chunk=>process.stderr.write(chunk));let output='';
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
for(const mode of ['none','warning','error'])app.get('/'+mode+'/:status/:body',{name:'Defaults'+mode,summary:'Defaults',options:{responseValidation:mode},versions:{first:{query:{match:z.enum(['both','first'])},responses:{200:z.string()}},last:{query:{match:z.enum(['both','last'])},responses:{201:z.string()}}}},(req,res)=>{res.setHeader('x-selected',req.version);res.status(Number(req.params.status));if(req.params.body==='empty')res.send();else if(req.params.body==='jsonstring')res.json('message');else if(req.params.body==='object')res.json({secret:'DO_NOT_LEAK'});else res.send('message');});
for(const mode of ['none','warning','error'])app.get('/unmatched/'+mode+'/:status',{name:'Unmatched'+mode,summary:'Unmatched',options:{requestValidation:mode,responseValidation:'error'},versions:{only:{query:{match:z.literal('yes')},responses:{200:z.string()}}}},(req,res)=>res.status(Number(req.params.status)).send('PRIVATE_HANDLER_BODY'));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','version_default_errors']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_default_errors'),[]);
 let count=0;const outcomes=[];let pairs=0;
 for(const mode of ['none','warning','error'])for(const match of ['both','first','last'])for(const status of [401,403,404,500])for(const body of ['text','jsonstring','empty','object']) {
 const response=await fetch(ts+'/'+mode+'/'+status+'/'+body+'?match='+match,{headers:{'x-correlation-id':'default-version-oracle'}});
 const result={mode,match,status,body,actualStatus:response.status,media:response.headers.get('content-type'),selected:response.headers.get('x-selected'),text:await response.text()};
 assert.equal(result.selected,match==='first'?'first':'last');
 assert.equal(result.actualStatus,mode==='error'&&body==='object'?500:status);
 if(body==='text'||body==='jsonstring')assert.equal(result.text,'message');
 if(body==='empty')assert.equal(result.text,'');
 if(mode==='error'&&body==='object')assert.ok(result.text.startsWith('Invalid response:')&&!result.text.includes('DO_NOT_LEAK'));
 if(mode==='error')for(const surface of ['native','local']) {
 const actualResponse=await fetch(rust+'/'+surface+'/'+mode+'/'+status+'/'+body+'?match='+match,{headers:{'x-correlation-id':'default-version-oracle'}});
 const actual={actualStatus:actualResponse.status,selected:actualResponse.headers.get('x-selected'),text:await actualResponse.text()};
 if(body!=='empty')assert.equal(actualResponse.headers.get('content-type'),result.media);
 assert.deepEqual(actual,{actualStatus:result.actualStatus,selected:result.selected,text:result.text},JSON.stringify({surface,result,actual}));pairs++;
 }
 outcomes.push(result);count++;
 }
 for(const match of ['both','first','last'])for(const status of [401,403,404,500]) {
 const path='/error/'+status+'/text?match='+match;
 const expectedResponse=await fetch(ts+path,{method:'HEAD'});
 for(const surface of ['native','local']) {
 const actual=await fetch(rust+'/'+surface+path,{method:'HEAD'});
 assert.equal(actual.status,expectedResponse.status);assert.equal(await actual.text(),'');assert.equal(actual.headers.get('x-selected'),expectedResponse.headers.get('x-selected'));pairs++;
 }
 }
 let unmatched=0;
 for(const mode of ['none','warning','error'])for(const match of ['yes','invalid'])for(const status of [401,403,404,500]) {
 const response=await fetch(ts+'/unmatched/'+mode+'/'+status+'?match='+match,{headers:{'x-correlation-id':'unmatched-oracle'}});const body=await response.text();
 assert.equal(response.status,match==='invalid'&&mode==='error'?400:status);
 if(match==='yes')assert.equal(body,'PRIVATE_HANDLER_BODY');
 else {assert.ok(!body.includes('PRIVATE_HANDLER_BODY'));if(mode!=='error')assert.equal(body,(status<500?'Request could not be processed:':'Internal server error:')+'\n\nCorrelation id: unmatched-oracle');}
 unmatched++;
 }
 console.log('PASS '+unmatched+' actual TypeScript no-match request-mode observations (native profile rejects None/Warning registration)');
 console.log('PASS '+pairs+' native/local versioned default comparison pairs');console.log('PASS '+count+' actual TypeScript versioned default status outcomes');
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
