import assert from 'node:assert/strict';
import {get} from 'node:http';
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
import {safeStringify} from '@forklaunch/common';
import {z} from 'zod/v3';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('headers','silent'),{mcp:false});
const cases={cookies:['set-cookie',['a=1; Path=/','b=2; Path=/']],accept:['accept',['application/json','text/plain']],duplicate:['x-duplicate',['one','two']],scalar:['x-scalar','plain'],object:['x-object',{value:123}],mixed:['x-mixed',['first',2,{value:true}]],empty:['x-empty',[]],crlf:['x-invalid',['safe','unsafe\\r\\nInjected: yes']],scalarcrlf:['x-invalid','unsafe\\nInjected: yes']};
function serve(req,res,raw){const [name,value]=cases[req.params.kind];try{res.setHeader(name,raw?(Array.isArray(value)?value.map(safeStringify):safeStringify(value)):value);res.json({ok:true});}catch(error){res.status(400).json({code:error.code});}}
app.internal.get('/reference/:kind',(req,res)=>serve(req,res,true));
for(const mode of ['none','warning','error'])app.get('/'+mode+'/:kind',{name:'Headers'+mode,summary:'Headers',params:{kind:z.string()},options:{responseValidation:mode},responses:{200:z.unknown(),400:z.unknown()}},(req,res)=>serve(req,res,false));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 function request(path){return new Promise((resolve,reject)=>get(ts+path,res=>{let text='';res.setEncoding('utf8');res.on('data',chunk=>text+=chunk);res.on('end',()=>{const headers=[];for(let i=0;i<res.rawHeaders.length;i+=2){const name=res.rawHeaders[i].toLowerCase();if(name==='set-cookie'||name==='accept'||name.startsWith('x-')&&!['x-powered-by','x-correlation-id'].includes(name))headers.push([name,res.rawHeaders[i+1]]);}resolve({status:res.statusCode,headers,body:JSON.parse(text)});});}).on('error',reject));}
 let count=0;
 for(const mode of ['none','warning','error'])for(const kind of ['cookies','accept','duplicate','scalar','object','mixed','empty','crlf','scalarcrlf'])for(let repeat=0;repeat<2;repeat++){
 const reference=await request('/reference/'+kind);const actual=await request('/'+mode+'/'+kind);assert.deepEqual(actual,reference,JSON.stringify({mode,kind,repeat}));
 if(kind.includes('crlf')){assert.equal(actual.status,400);assert.equal(actual.body.code,'ERR_INVALID_CHAR');}else assert.equal(actual.status,200);
 if(kind==='cookies')assert.equal(actual.headers.filter(([key])=>key==='set-cookie').length,2);
 if(kind==='empty')assert.equal(actual.headers.length,0);
 count++;}
 console.log(`PASS ${count} actual ForkLaunch/Node raw response-header comparison pairs, including repeated Set-Cookie, arrays, scalar/object, empty arrays and rejected CRLF`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
