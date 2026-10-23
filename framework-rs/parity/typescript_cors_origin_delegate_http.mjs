// Actual ForkLaunch HTTP oracle and optional native CORS origin delegate comparison.
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
const temporary=mkdtempSync(join(tmpdir(),'fl-router-origin-'));
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
import {SchemaValidator,string} from '@forklaunch/validator/zod';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('cors','silent'),{mcp:false,cors:{credentials:true,origin(origin,callback){const mode=process.argv[2];const value=mode==='true'||mode==='async-true'?true:mode==='fixed'?'https://fixed.example':mode==='list'?['https://allowed.example']:mode==='empty'?'':false;const error=mode.includes('error')?new Error('PRIVATE_DELEGATE'):null;if(mode.startsWith('async'))setImmediate(()=>callback(error,value));else callback(error,value);}}});
app.get('/param/:id',{name:'Param',summary:'Param',params:{id:string},responses:{200:string}},(_req,res)=>res.send('ok'));
app.get('/ok',{name:'Ok',summary:'Ok',responses:{200:string}},(_req,res)=>res.setHeader('x-handler','get').send('ok'));
app.get('/error',{name:'Error',summary:'Error',responses:{200:string}},()=>{throw new Error('PRIVATE');});
app.options('/explicit',{name:'Options',summary:'Options',responses:{202:string}},(_req,res)=>res.setHeader('x-handler','options').status(202).send('explicit'));
app.get('/override',{name:'Override',summary:'Override',responses:{200:string}},(_req,res)=>res.setHeader('access-control-allow-origin','application').setHeader('vary','Accept-Encoding').send('override'));
const server=app.listen(0,'127.0.0.1');
server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));
process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);


 const native=process.env.FORKLAUNCH_CORS_DELEGATE_NATIVE==='1';
 if(native)run('cargo',['build','--locked','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','cors_parity']);
 const rows=[];
 for(const mode of ['true','false','fixed','list','empty','error','async-true','async-error']){
  const url=await start(process.execPath,['service.mjs',mode]);
  const rust=native?await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/cors_parity'),[JSON.stringify({delegate:mode,credentials:true})]):null;
  for(const origin of [undefined,'','https://allowed.example','https://denied.example'])for(const method of ['GET','OPTIONS']){
   const response=await fetch(url+'/ok',{method,headers:{...(origin===undefined?{}:{origin}),'x-correlation-id':'delegate-fixture'}});
   const body=await response.text();
   const headers=Object.fromEntries([...response.headers].filter(([key])=>key.startsWith('access-control')||['vary','x-correlation-id','content-type'].includes(key)));
   assert.equal(response.status,mode.includes('error')?500:method==='OPTIONS'&&!['false','empty'].includes(mode)?204:200);
   if(mode.includes('error')){assert.equal(body,'Internal server error:\n\nCorrelation id: No correlation ID');assert.equal(headers['access-control-allow-origin'],undefined);assert.equal(headers['content-type'],'text/plain; charset=utf-8');}
   assert.equal(headers['x-correlation-id'],'delegate-fixture');
   if(!mode.includes('error')){
    const disabled=['false','empty'].includes(mode);
    assert.equal(headers['access-control-allow-credentials'],disabled?undefined:'true');
    const allowed=disabled?undefined:mode==='fixed'?'https://fixed.example':mode==='list'&&origin!=='https://allowed.example'?undefined:origin||undefined;
    assert.equal(headers['access-control-allow-origin'],allowed);
    if(disabled)assert.equal(headers.vary,undefined);
   }
   if(rust){
    const response=await fetch(rust+'/ok',{method,headers:{...(origin===undefined?{}:{origin}),'x-correlation-id':'delegate-fixture'}});
    const observed={status:response.status,headers:Object.fromEntries([...response.headers].filter(([key])=>key.startsWith('access-control')||['vary','x-correlation-id','content-type'].includes(key))),body:await response.text()};
    assert.deepEqual(observed,{status:mode.includes('error')?500:method==='OPTIONS'&&!['false','empty'].includes(mode)?204:200,headers,body},JSON.stringify({mode,origin,method}));
   }
   rows.push({mode,origin:origin??null,method,status:response.status,headers,body});
  }
 }
 console.log(JSON.stringify({cases:rows.length,rows},null,2));
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
