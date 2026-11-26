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
const options=JSON.parse(process.argv[2]);
if(Object.prototype.hasOwnProperty.call(options,'maxAgeNumber')){options.maxAge=Number(options.maxAgeNumber);delete options.maxAgeNumber;}
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('cors','silent'),{mcp:false,cors:options});
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

 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','cors_parity']);
 let comparisons=0;let noContentLengthDifferences=0;
 const numericAges=[-Number.MAX_VALUE,-1,-0.5,-0,0,0.5,Number.MAX_SAFE_INTEGER+1,1e21,Number.MAX_VALUE,'Infinity','-Infinity','NaN'];
 const configs=[...numericAges.map(maxAgeNumber=>({maxAgeNumber})),{}, {origin:'https://fixed.example'}, {origin:true}, {origin:['https://allowed.example']}, {origin:false}, {credentials:true}, {origin:['https://allowed.example'],credentials:true}, {methods:['GET','PATCH'],allowedHeaders:['X-One','X-Two'],exposedHeaders:['X-Result'],maxAge:0}, {preflightContinue:true}, ...[200,205,299,300,304,400,418,500,599].map(optionsSuccessStatus=>({optionsSuccessStatus}))];
 for(const config of configs){
  const ts=await start(process.execPath,['service.mjs',JSON.stringify(config)]);
  const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/cors_parity'),[JSON.stringify(config)]);
  for(const input of [{},{origin:'https://allowed.example'},{origin:'https://denied.example'},{origin:'null'},{origin:'https://allowed.example','access-control-request-method':'PATCH','access-control-request-headers':'X-One, x-two'}])for(const [method,path] of [['GET','/ok'],['HEAD','/ok'],['GET','/error'],['GET','/missing'],['OPTIONS','/explicit'],['OPTIONS','/ok'],['OPTIONS','/missing'],['OPTIONS','/param/%GG'],['GET','/override']]){
   const results=[];
   for(const url of [ts,rust]){
    const response=await fetch(url+path,{method,headers:{...input,'x-correlation-id':'cors-fixture'}});
    results.push({status:response.status,headers:Object.fromEntries([...response.headers].filter(([key])=>key.startsWith('access-control')||['vary','x-handler','content-length','allow'].includes(key))),body:await response.text()});
   }
   if([204,304].includes(results[0].status)){
    assert.equal(results[0].headers['content-length'],'0','Express cors explicitly sends zero');
    assert.equal(results[1].headers['content-length'],undefined,'Hyper removes zero Content-Length on204/304');
    delete results[0].headers['content-length'];noContentLengthDifferences++;
   }
   assert.deepEqual(results[1],results[0],JSON.stringify({config,input,method,path}));comparisons++;

  }
 }
 console.log(`PASS ${comparisons} actual TypeScript/native static CORS comparisons (${comparisons*2} HTTP observations); ${comparisons-noContentLengthDifferences} exact pairs, ${noContentLengthDifferences} pairs with explicitly asserted 204/304 zero Content-Length transport difference`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
