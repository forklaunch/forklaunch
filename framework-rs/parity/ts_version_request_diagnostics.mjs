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
const validator=SchemaValidator();const originalParse=validator.parse.bind(validator);
validator.parse=(schema,value)=>{if(typeof value==='string'&&value.startsWith('Version '))throw new Error('Framework diagnostic entered application response validator');return originalParse(schema,value);};
const app=forklaunchExpress(validator,new OpenTelemetryCollector('queryversions','silent'),{mcp:false});
import assert from 'node:assert/strict';
import {isValidationDiagnostic,sendValidationDiagnostic} from './source/core/src/http/middleware/internal/validationDiagnostic';
const first={},second={};
assert.throws(()=>sendValidationDiagnostic(first,()=>{assert.equal(isValidationDiagnostic(first),true);assert.equal(isValidationDiagnostic(second),false);sendValidationDiagnostic(first,()=>{});assert.equal(isValidationDiagnostic(first),true);throw new Error('sentinel');}));
assert.equal(isValidationDiagnostic(first),false);
let handlerCalls=0;
app.get('/counts',{name:'Counts',summary:'Counts',responses:{200:z.unknown()}},(req,res)=>res.json({handlerCalls}));
for(const requestValidation of ['none','warning','error'])for(const responseValidation of ['none','warning','error'])for(const auth of [false,true])app.get('/'+requestValidation+'/'+responseValidation+'/'+auth,{name:requestValidation+responseValidation+auth,summary:'Diagnostic',...(auth?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),options:{requestValidation,responseValidation},versions:{one:{query:{tag:z.literal('one')},responses:{200:z.unknown()}},two:{query:{tag:z.literal('two')},responses:{200:z.unknown()}}}},(req,res)=>{handlerCalls++;res.json({handler:true,version:req.version??null});});
for(const responseValidation of ['none','warning','error'])app.get('/invalid/'+responseValidation,{name:'Invalid'+responseValidation,summary:'Invalid response',options:{responseValidation},versions:{one:{query:{tag:z.literal('one')},responses:{200:z.object({ok:z.literal(true)})}}}},(req,res)=>{handlerCalls++;res.json({ok:false});});
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 const ts=await start(process.execPath,['service.mjs']);
 let count=0,expectedCalls=0;const observations=[];
 const request=async pathname=>{const response=await fetch(ts+pathname,{headers:{'x-correlation-id':'diagnostic-audit'}});return {status:response.status,media:response.headers.get('content-type'),body:await response.text()};};
 const verifyCalls=async()=>assert.equal(JSON.parse((await request('/counts')).body).handlerCalls,expectedCalls);
 for(const requestMode of ['none','warning','error'])for(const responseMode of ['none','warning','error'])for(const tag of ['bad','one','bad','two']){
  const pathname='/'+requestMode+'/'+responseMode+'/false?tag='+tag;
  const result=await request(pathname);
  observations.push({pathname,result,handler:!(tag==='bad'&&requestMode==='error'),protectedRequest:false,requestMode});
  if(tag==='bad'&&requestMode==='error') {
   assert.equal(result.status,400);assert.equal(result.media,'text/plain; charset=utf-8');
   assert.ok(result.body.startsWith('Version one request validation failed'));
   assert.ok(result.body.includes('Version two request validation failed'));
   assert.ok(result.body.endsWith('Correlation id: diagnostic-audit'));
  }else {
   expectedCalls++;
   const rejected=tag==='bad'&&responseMode==='error';assert.equal(result.status,rejected?500:200);
   if(!rejected)assert.deepEqual(JSON.parse(result.body),{handler:true,version:tag==='bad'?null:tag});
   else assert.equal(result.body,'Internal server error:\n\nCorrelation id: diagnostic-audit');
  }
  await verifyCalls();count++;
 }
 for(const requestMode of ['none','warning','error'])for(const responseMode of ['none','warning','error']){
  const pathname='/'+requestMode+'/'+responseMode+'/true?tag=bad';
  const result=await request(pathname);
  observations.push({pathname,result,handler:false,protectedRequest:true,requestMode});
  assert.equal(result.status,requestMode==='error'?400:401);await verifyCalls();count++;
 }
 for(const responseMode of ['none','warning','error'])for(let repeat=0;repeat<3;repeat++){
  const result=await request('/invalid/'+responseMode+'?tag=one');expectedCalls++;
  assert.equal(result.status,responseMode==='error'?500:200);
  if(responseMode==='error')assert.ok(result.body.startsWith('Invalid response:'));
  else assert.deepEqual(JSON.parse(result.body),{ok:false});
  await verifyCalls();count++;
 }
 if(process.env.FORKLAUNCH_VERSION_DIAGNOSTICS_NATIVE==='1') {
  run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','version_request_modes']);
  const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_request_modes'),[]);
  let handlerCalls=0,exact=0,precedence=0,diagnostics=0;
  for(const observation of observations)for(const surface of ['native','local']) {
   const response=await fetch(rust+'/'+surface+observation.pathname,{headers:{'x-correlation-id':'diagnostic-audit'}});
   const body=await response.text();
   const authBeforeSchema=process.env.FORKLAUNCH_VERSION_PREAUTH!=='1'&&observation.protectedRequest&&observation.requestMode==='error';
   assert.equal(response.status,authBeforeSchema?401:observation.result.status,JSON.stringify({surface,observation,body}));
   if(authBeforeSchema)precedence++;
   if(response.status===200){assert.deepEqual(JSON.parse(body),JSON.parse(observation.result.body));exact++;}
   else if(body!==observation.result.body)diagnostics++;
   if(observation.handler)handlerCalls++;
   assert.equal((await (await fetch(rust+'/counts')).json()).handlerCalls,handlerCalls);
  }
  assert.equal(precedence,process.env.FORKLAUNCH_VERSION_PREAUTH==='1'?0:6);
  console.log(`PASS native mode characterization: ${exact} exact successes, ${precedence} explicit auth-before-version status differences, ${diagnostics} existing diagnostic differences (including the ${precedence} ordering cases); handler counts checked for all90 requests`);
 }
 console.log(`PASS ${count} actual Express version diagnostic cases: repeated failures/successes, all validation modes, preserved authentication and handler-response validation; scoped marker cleanup passed`);
}finally{
 for(const child of children){if(child.exitCode!==null||child.signalCode!==null)continue;await new Promise(resolve=>{child.once('exit',resolve);child.kill('SIGTERM');});}
 rmSync(temporary,{recursive:true,force:true});
}
