import {request as httpRequest} from 'node:http';
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
const temporary=mkdtempSync(join(tmpdir(),'fl-correlation-'));
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
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('correlation','silent'),{mcp:false});
app.get('/context',{name:'Context',summary:'Context',responses:{200:string}},(req,res)=>res.send(req.context.correlationId));
app.get('/override',{name:'Override',summary:'Override',responses:{200:string}},(req,res)=>res.setHeader('x-correlation-id','application').send(req.context.correlationId));
app.get('/throw',{name:'Throw',summary:'Throw',responses:{200:string}},()=>{throw new Error('PRIVATE');});
app.get('/auth',{name:'Auth',summary:'Auth',access:'authenticated',auth:{jwt:{signatureKey:'test-only'}},responses:{200:string}},(_req,res)=>res.send('ok'));
app.get('/param/:id',{name:'Param',summary:'Param',params:{id:string},responses:{200:string}},(_req,res)=>res.send('ok'));
app.post('/parse',{name:'Parse',summary:'Parse',body:{value:string},responses:{200:string}},(_req,res)=>res.send('ok'));
const server=app.listen(0,'127.0.0.1');
server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));
process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','correlation_parity']);
 const ts=await start(process.execPath,['service.mjs']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/correlation_parity'),[]);

 const variants=[{}, {'x-request-id':'request-only'}, {'x-correlation-id':'supplied','x-request-id':'independent'}, {'x-correlation-id':'','x-request-id':'request-only'}, {'x-correlation-id':' \t '}, {'x-correlation-id':'  padded  '}, {'x-correlation-id':['first','second']}, {'x-correlation-id':['','second']}, {'x-correlation-id':'0'}, ...[0x80,0xe9,0xff].map(byte=>({'x-correlation-id':'caf'+String.fromCharCode(byte)}))];
 const uuid=/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
 let comparisons=0;
 for(const headers of variants)for(const [method,path] of [['GET','/missing'],['HEAD','/missing'],['GET','/context'],['HEAD','/context'],['GET','/override'],['GET','/throw'],['GET','/auth'],['GET','/param/%GG'],['POST','/parse']]){
  const results=[];
  for(const url of [ts,rust]){
   const result=await new Promise((resolve,reject)=>{
    const body=method==='POST'?'{':'';
    const request=httpRequest(url+path,{method,headers:{...headers,...(body?{'content-type':'application/json','content-length':String(Buffer.byteLength(body))}:{})}},response=>{let body='';response.setEncoding('utf8');response.on('data',chunk=>body+=chunk);response.on('end',()=>resolve({status:response.statusCode,headers:response.headers,body}));});request.on('error',reject);request.end(body);
   });
   const generated=!headers['x-correlation-id'] || (typeof headers['x-correlation-id']==='string' && !headers['x-correlation-id'].trim());
   let correlation=result.headers['x-correlation-id'];
   assert.ok(correlation,`${method} ${path} correlation header`);
   if(generated){
    if(path==='/override'){assert.equal(correlation,'application');assert.match(result.body,uuid);result.body='GENERATED';}
    else {assert.match(correlation,uuid);result.body=result.body.replaceAll(correlation,'GENERATED');correlation='GENERATED';}
   }
   if(url===rust && headers['x-request-id'])assert.equal(result.headers['x-request-id'],headers['x-request-id']);
   // Authentication envelope profiles are independent; check correlation + rejection.
   results.push({status:result.status,correlation,body:path==='/auth'?'AUTH_REJECTION':result.body});
  }
  assert.deepEqual(results[1],results[0],`${method} ${path} ${JSON.stringify(headers)}`);comparisons++;
 }
 console.log(`PASS ${comparisons} actual TypeScript/native correlation comparisons (${comparisons*2} HTTP observations), including raw Latin-1 request headers`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
