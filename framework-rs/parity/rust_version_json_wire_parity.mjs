import {connect} from 'node:net';
import {mcpRpc} from './rust_mcp_probe.mjs';
import {gzipSync} from 'node:zlib';
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
const diagnosticProfile=process.env.FORKLAUNCH_DIAGNOSTIC_NATIVE==='1';
if(diagnosticProfile)process.env.FORKLAUNCH_ZOD_DIAGNOSTIC_SCHEMAS=join(temporary,'diagnostic-schemas.json');
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
import {writeFileSync} from 'node:fs';
import {SchemaValidator} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
const diagnosticSchemas={};
let handlers=0,verifiers=0;const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('versions','silent'),{mcp:false,json:{limit:128,verify(req){verifiers++;if(req.headers['x-deny']==='yes')throw new Error('PRIVATE_VERIFY');}}});
for(const mode of ['none','warning','error'])for(const optional of [false,true]){const body=optional?z.object({flag:z.literal('ok')}).optional().default({flag:'ok'}):z.object({flag:z.literal('ok')});if(process.env.FORKLAUNCH_ZOD_DIAGNOSTIC_SCHEMAS)diagnosticSchemas[optional]=SchemaValidator().openapi2020(body,{zodDiagnostics:true,diagnosticDefaults:new Map(optional?[[body,{flag:'ok'}]]:[])});app.post('/'+mode+'/'+optional,{name:'Body'+mode+optional,summary:'Body',options:{requestValidation:mode,responseValidation:'none'},versions:{first:{body,responses:{200:z.unknown()}},last:{body,responses:{200:z.unknown()}}}},(req,res)=>{handlers++;res.json({selected:req.version??null,body:req.body??null});});}
if(process.env.FORKLAUNCH_ZOD_DIAGNOSTIC_SCHEMAS)writeFileSync(process.env.FORKLAUNCH_ZOD_DIAGNOSTIC_SCHEMAS,JSON.stringify(diagnosticSchemas));
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:z.unknown()}},(_req,res)=>res.json({handlers,verifiers}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--features','mcp','--example','version_json_wire']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_json_wire'),[]);
 const cases=[];
 for(const media of ['application/json','text/plain','application/problem+json','application/json; charset=bogus'])for(const body of ['{"flag":"ok"}','{',''])cases.push({media,body:Buffer.from(body),framing:'length'});
 cases.push(...[
 {media:'application/json',body:Buffer.alloc(0),framing:'none'},
 {media:'application/json',body:Buffer.alloc(0),framing:'chunked'},
 {media:'application/json',body:Buffer.from('null'),framing:'length'},
 {media:'application/json',body:Buffer.from('   '),framing:'length'},
 {media:'application/json',body:Buffer.from('{"flag":"ok"}'),framing:'length',headers:{'x-deny':'yes'}},
 {media:'text/plain',body:Buffer.from('{'),framing:'length',headers:{'x-deny':'yes','content-encoding':'bogus'}},
 {media:'application/json',body:Buffer.from(' '.repeat(129)),framing:'length'},
 {media:'application/json',body:gzipSync('{"flag":"ok"}'),framing:'length',headers:{'content-encoding':'gzip'}},
 ]);
 async function wire(url,path,input) {
 const address=new URL(url);const before=await(await fetch(url+'/calls')).json();
 const raw=await new Promise((resolve,reject)=>{const chunks=[];const socket=connect(Number(address.port),address.hostname,()=>{
 const headers={'content-type':input.media,'x-correlation-id':'version-body',...input.headers,...(input.framing==='length'?{'content-length':input.body.length}:input.framing==='chunked'?{'transfer-encoding':'chunked'}:{})};
 const head='POST '+path+' HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n'+Object.entries(headers).map(([key,value])=>key+': '+value+'\r\n').join('')+'\r\n';
 const bytes=input.framing==='chunked'?Buffer.from('0\r\n\r\n'):input.body;
 socket.write(Buffer.concat([Buffer.from(head),bytes]));});socket.on('data',chunk=>chunks.push(chunk));socket.on('end',()=>resolve(Buffer.concat(chunks)));socket.on('error',reject);});
 const split=raw.indexOf('\r\n\r\n');const status=Number(raw.toString('latin1').split(' ')[1]);const text=raw.subarray(split+4).toString();const after=await(await fetch(url+'/calls')).json();
 return {status,body:status===200?JSON.parse(text):text,handlers:after.handlers-before.handlers,verifiers:after.verifiers-before.verifiers};
 }
 let exact=0,diagnostics=0,requests=0;
 for(const mode of ['none','warning','error'])for(const optional of [false,true])for(const input of cases) {
 const path='/'+mode+'/'+optional;const expected=await wire(ts,path,input);requests++;
 assert.equal(expected.handlers,expected.status===200?1:0);
 const mediaMatch=input.media==='application/json';
 assert.equal(expected.verifiers,mediaMatch&&input.framing!=='none'&&input.body.length<=128?1:0,JSON.stringify({input,expected}));
 if(input.media.includes('bogus'))assert.equal(expected.status,415);
 if(input.headers?.['x-deny']==='yes'&&mediaMatch)assert.equal(expected.status,403);
 if(!mediaMatch&&optional&&!input.media.includes('bogus')){assert.equal(expected.status,200);assert.deepEqual(expected.body,{selected:'last',body:{flag:'ok'}});}
 
 for(const surface of ['native','local']) {
 const actual=await wire(rust,'/'+surface+path,input);const label=JSON.stringify({mode,optional,surface,input,expected,actual});
 assert.equal(actual.status,expected.status,label);assert.equal(actual.handlers,expected.handlers,label);assert.equal(actual.verifiers,expected.verifiers,label);
 if(!diagnosticProfile&&expected.status===400&&String(expected.body).startsWith('Version first request')) {
 assert.ok(String(actual.body).includes('request does not match any declared API version'),label);diagnostics++;
 } else {assert.deepEqual(actual,expected,label);exact++;}
 assert.ok(!JSON.stringify(actual).includes('PRIVATE_VERIFY'));
 }
 }
 const listed=await mcpRpc(rust,'tools/list');const bodyTools=listed.tools.filter(tool=>tool.name!=='calls');assert.equal(bodyTools.length,24);
 for(const tool of bodyTools) {
 const before=await(await fetch(rust+'/calls')).json();
 const result=await mcpRpc(rust,'tools/call',{name:tool.name,arguments:{body:{flag:'ok'}}});
 assert.equal(result.isError,false,JSON.stringify({tool,result}));assert.deepEqual(JSON.parse(result.content[0].text),{selected:'last',body:{flag:'ok'}});
 const after=await(await fetch(rust+'/calls')).json();assert.equal(after.verifiers-before.verifiers,1);assert.equal(after.handlers-before.handlers,1);
 }
 console.log('PASS '+bodyTools.length+' real MCP tool calls preserve supplied JSON bodies with exactly one verifier and handler invocation');
 console.log('PASS '+requests+' actual TypeScript request cases; '+exact+' exact native/local pairs, '+diagnostics+' explicit schema-diagnostic differences; all-mode status/verify/handler counts and unmatched parser projections match');
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
