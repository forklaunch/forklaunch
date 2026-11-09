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
const temporary=mkdtempSync(join(tmpdir(),'fl-canonical-http-'));
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
  '@fixture/openapi':['source/core/src/http/openApiV3Generator/openApiV3Generator.ts'],
  '@forklaunch/common':['source/common/index.ts'],'@forklaunch/validator':['source/validator/index.ts'],
  '@forklaunch/validator/*':['source/validator/src/*/index.ts'],'@forklaunch/core/*':['source/core/src/*/index.ts'],
  '@forklaunch/express':['source/express/index.ts']},target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true}}));
 const fixtureSource=readFileSync(join(fixture,'service.ts'),'utf8').replace('{mcp:false,json:', '{mcp:false,validation:{response:\'warning\'},json:');
 const routes=`
const statusWarnings: string[]=[];
const statusCollector=new OpenTelemetryCollector('status-oracle','silent');
statusCollector.warn=(message: unknown)=>{statusWarnings.push(String(message));};
for(const mode of ['error','warning','none','global'] as const)for(const status of [201,301,418,503,200])app.get('/oracle/'+mode+'/'+status,{
 name:'Oracle'+mode+status,summary:'Oracle',options:{openapi:false,responseValidation:mode==='global'?undefined:mode},responses:{200:string}
},(req,res)=>{req.openTelemetryCollector=statusCollector;res.type('text/plain').status(status).send('public value');});
for(const mode of ['error','warning','none','global'] as const)app.get('/oracle/schema/'+mode,{
 name:'Schema'+mode,summary:'Schema',options:{openapi:false,responseValidation:mode==='global'?undefined:mode},responses:{200:string}
},(req,res)=>{req.openTelemetryCollector=statusCollector;res.status(200).json(42);});
app.get('/oracle/warnings',{name:'Warnings',summary:'Warnings',options:{openapi:false},responses:{200:array(string)}},(_req,res)=>res.json(statusWarnings));
`;
 writeFileSync(join(temporary,'service.ts'),fixtureSource.replace('let queryCalls=0;',routes+'\nlet queryCalls=0;'));
 cpSync(join(fixture,'openapi.ts'),join(temporary,'openapi.ts'));
 run(join(dependencies,'node_modules/.bin/esbuild'),['openapi.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=openapi.mjs']);
 run(process.execPath,['openapi.mjs']);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 const ts=await start(process.execPath,['service.mjs']);
 let cases=0;
 for(const mode of ['error','warning','none','global'])for(const status of [201,301,418,503,200])for(let attempt=0;attempt<2;attempt++){
  const response=await fetch(`${ts}/oracle/${mode}/${status}`,{redirect:'manual',headers:{'x-correlation-id':'status-mode'}});
  const rejected=mode==='error'&&status!==200;
  const expectedStatus=rejected?(status>=400?status:500):status;
  assert.equal(response.status,expectedStatus);
  assert.equal(response.headers.get('content-type'),'text/plain; charset=utf-8');
  assert.equal(await response.text(),rejected?`${expectedStatus>=500?'Internal server error':'Request could not be processed'}:\n\nCorrelation id: status-mode`:'public value');
  cases++;
 }
 for(const mode of ['error','warning','none','global'])for(let attempt=0;attempt<2;attempt++){
  const response=await fetch(`${ts}/oracle/schema/${mode}`,{headers:{'x-correlation-id':'status-mode'}});
  assert.equal(response.status,mode==='error'?500:200);
  const body=await response.text();
  if(mode==='error')assert.match(body,/^Invalid response:/);else assert.equal(body,'42');
  cases++;
 }
 const warnings=await (await fetch(ts+'/oracle/warnings')).json();
 assert.equal(warnings.length,20);
 for(const status of [201,301,418,503])assert.equal(warnings.filter(message=>message.includes(`Undocumented response status ${status}`)).length,4);
 assert.equal(warnings.filter(message=>!message.includes('Undocumented')).length,4);
 console.log(`PASS ${cases} actual-source Error/Warning/None repeated response outcomes and twenty warning emissions`);
} finally {
 for(const child of children)child.kill('SIGTERM');
 rmSync(temporary,{recursive:true,force:true});
}
