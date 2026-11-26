import {connect} from 'node:net';
import {mcpRpc} from './rust_mcp_probe.mjs';
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
import {SchemaValidator} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {forklaunchExpress} from '@forklaunch/express';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('absence','silent'),{mcp:false});
let handlers=0,transforms=0;
for(const kind of ['json','text'])for(const mode of ['none','warning','error'])for(const form of ['required','optional','default','transform'])for(const method of ['post','delete']) {
 let schema:any=kind==='json'?z.object({flag:z.string()}):z.literal('ok');
 if(form!=='required')schema=schema.optional();
 if(form==='default')schema=schema.default(kind==='json'?{flag:'default'}:'ok');
 if(form==='transform')schema=schema.transform((value:any)=>{transforms++;return value===undefined?(kind==='json'?{flag:'transformed'}:'transformed'):value;});
 (app as any)[method]('/'+[kind,mode,form].join('/'),{name:[kind,mode,form,method].join('_'),summary:'Absence',options:{requestValidation:mode,responseValidation:'none'},versions:{first:{body:{[kind]:schema},responses:{200:z.unknown()}},last:{body:{[kind]:schema},responses:{200:z.unknown()}}}},(req:any,res:any)=>{handlers++;res.json({selected:req.version??null,present:req.body!==undefined,body:req.body??null});});
}
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:z.unknown()}},(_req:any,res:any)=>res.json({handlers,transforms}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--features','mcp','--example','version_optional_body']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_optional_body'),[]);
 async function wire(url,path,input) {
 const address=new URL(url);const before=await(await fetch(url+'/calls')).json();
 const raw=await new Promise((resolve,reject)=>{const chunks=[];const socket=connect(Number(address.port),address.hostname,()=>{
 const headers={'content-type':input.media,'x-correlation-id':'version-body',...input.headers,...(input.framing==='length'?{'content-length':input.body.length}:input.framing==='chunked'?{'transfer-encoding':'chunked'}:{})};
 const head=input.method+' '+path+' HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n'+Object.entries(headers).map(([key,value])=>key+': '+value+'\r\n').join('')+'\r\n';
 const bytes=input.framing==='chunked'?Buffer.from('0\r\n\r\n'):input.body;
 socket.write(Buffer.concat([Buffer.from(head),bytes]));});socket.on('data',chunk=>chunks.push(chunk));socket.on('end',()=>resolve(Buffer.concat(chunks)));socket.on('error',reject);});
 const split=raw.indexOf('\r\n\r\n');const status=Number(raw.toString('latin1').split(' ')[1]);const text=raw.subarray(split+4).toString();const after=await(await fetch(url+'/calls')).json();
 return {status,body:status===200?JSON.parse(text):text,handlers:after.handlers-before.handlers,transforms:after.transforms-before.transforms};
 }
 let exact=0,diagnostics=0,requests=0;
 for(const kind of ['json','text'])for(const mode of ['none','warning','error'])for(const form of ['required','optional','default','transform'])for(const method of ['POST','DELETE'])for(const test of ['unframed','empty','wrongmedia','valid','null']) {
 const input={method,media:test==='wrongmedia'?'application/octet-stream':kind==='json'?'application/json':'text/plain',body:Buffer.from(test==='valid'?(kind==='json'?'{"flag":"ok"}':'ok'):test==='null'?'null':''),framing:test==='unframed'?'none':'length'};
 const path='/'+[kind,mode,form].join('/');const expected=await wire(ts,path,input);requests++;
 assert.equal(expected.handlers,expected.status===200?1:0);
 for(const surface of ['native','local']) {
 const actual=await wire(rust,'/'+surface+path,input);const label=JSON.stringify({kind,mode,form,method,test,surface,expected,actual});
 assert.equal(actual.status,expected.status,label);assert.equal(actual.handlers,expected.handlers,label);assert.equal(actual.transforms,expected.transforms,label);
 if(expected.status===400&&String(expected.body).startsWith('Version first request')) {assert.ok(String(actual.body).includes('request does not match any declared API version'),label);diagnostics++;}
 else {assert.deepEqual(actual,expected,label);exact++;}
 }
 }
 const listed=await mcpRpc(rust,'tools/list');
 let toolCalls=0;
 for(const tool of listed.tools.filter(tool=>tool.name.toLowerCase().includes('erroroptional'))) {
  assert.equal(tool.inputSchema.required?.includes('body')??false,false,tool.name);
  const result=await mcpRpc(rust,'tools/call',{name:tool.name,arguments:{}});
  const isDelete=tool.name.toLowerCase().includes('delete');
  assert.equal(!!result.isError,!isDelete,JSON.stringify({name:tool.name,result}));
  if(isDelete)assert.deepEqual(JSON.parse(result.content[0].text),{selected:'last',present:false,body:null});
  toolCalls++;
 }
 assert.equal(toolCalls,16);console.log('PASS16 optional native MCP declarations and framed POST/unframed DELETE omissions across both adapters/versions');
 run('cargo',['run','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','typescript_optional_body','--',temporary]);
 const docs=JSON.parse(readFileSync(join(temporary,'openapi.json'),'utf8'));
 assert.equal(docs.optional.paths['/presence'].post.requestBody.required,false);
 assert.equal(docs.required.paths['/presence'].post.requestBody.required,true);
 writeFileSync(join(temporary,'types.ts'),`
import {createForkLaunchSdk,OptionalPresencePresenceVersionOptionalRequest,OptionalPresencePresenceVersionRequiredRequest} from './client.js';
const absent:OptionalPresencePresenceVersionOptionalRequest={};
const supplied:OptionalPresencePresenceVersionOptionalRequest={body:{flag:'ok'}};
// @ts-expect-error optional absence is not nullable
const invalid:OptionalPresencePresenceVersionOptionalRequest={body:null};
// @ts-expect-error required version keeps mandatory body
const missing:OptionalPresencePresenceVersionRequiredRequest={};
const sdk=createForkLaunchSdk();if(false){sdk.optional.presence.presenceVersionOptional();sdk.optional.presence.presenceVersionOptional(absent);sdk.optional.presence.presenceVersionOptional(supplied);}
`);
 run(process.env.TSC_BIN??join(root,'framework/node_modules/.bin/tsc'),['--strict','--noEmit','--skipLibCheck','--target','ES2022','--module','NodeNext','--moduleResolution','NodeNext','types.ts','client.ts']);
 writeFileSync(join(temporary,'client-check.ts'),`
import assert from 'node:assert/strict';import {createForkLaunchSdk} from './client';
const calls:any[]=[];const sdk=createForkLaunchSdk({baseUrl:'http://127.0.0.1:9',fetch:async(_url:any,input:any)=>{calls.push(input);return new Response(null,{status:204});}});
await sdk.optional.presence.presenceVersionOptional();await sdk.optional.presence.presenceVersionOptional({body:{flag:'ok'}});
assert.equal(calls[0].body,undefined);assert.equal(calls[0].headers.get('content-type'),null);assert.equal(calls[1].body,'{"flag":"ok"}');
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['client-check.ts','--bundle','--platform=node','--format=esm','--packages=external','--outfile=client-check.mjs']);run(process.execPath,['client-check.mjs']);
 console.log('PASS optional contract/version artifacts, OpenAPI requiredness, SDK plan, strict TypeScript request types and absent/supplied SDK transport; Rust retains its existing Option body API');
 assert.equal(requests,240);assert.equal(exact+diagnostics,480);
 console.log(`PASS ${requests} actual optional-body cases, ${exact} exact native/local pairs and ${diagnostics} explicit schema-diagnostic differences; schema defaults/transforms and absent/null/framing remain distinct`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
