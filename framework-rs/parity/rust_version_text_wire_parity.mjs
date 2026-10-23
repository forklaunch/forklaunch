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
let handlers=0,verifiers=0;const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('versions','silent'),{mcp:false,text:{limit:128,verify(req){verifiers++;if(req.headers['x-deny']==='yes')throw new Error('PRIVATE_VERIFY');}}});
for(const mode of ['none','warning','error'])for(const optional of [false,true]){const body=optional?z.literal('ok').optional().default('ok'):z.literal('ok');for(const method of ['post','delete'])app[method]('/'+mode+'/'+optional,{name:'Body'+mode+optional,summary:'Body',options:{requestValidation:mode,responseValidation:'none'},versions:{first:{body:{text:body},responses:{200:z.unknown()}},last:{body:{text:body},responses:{200:z.unknown()}}}},(req,res)=>{handlers++;res.json({selected:req.version??null,body:req.body??null});});}
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:z.unknown()}},(_req,res)=>res.json({handlers,verifiers}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--features','mcp','--example','version_text_wire']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_text_wire'),[]);
 const cases=[];
 for(const media of ['text/plain','application/json','text/custom','text/plain; charset=bogus'])for(const body of ['ok','bad',''])cases.push({media,body:Buffer.from(body),framing:'length'});
 cases.push(
 {media:'text/plain',body:Buffer.alloc(0),framing:'none'},
 {media:'text/plain',body:Buffer.alloc(0),framing:'chunked'},
 {media:'text/plain',body:Buffer.from('ok'),framing:'length',headers:{'x-deny':'yes'}},
 {media:'application/json',body:Buffer.from('bad'),framing:'length',headers:{'x-deny':'yes','content-encoding':'bogus'}},
 {media:'text/plain',body:Buffer.from('x'.repeat(129)),framing:'length'},
 {media:'text/plain',body:gzipSync('ok'),framing:'length',headers:{'content-encoding':'gzip'}},
 {media:'text/plain; charset=utf-16le',body:Buffer.from('ok','utf16le'),framing:'length'},
 {media:'text/plain; charset=iso-8859-1',body:Buffer.from([0xe9]),framing:'length'},
 {media:'text/plain',body:Buffer.from([0xc3,0x28]),framing:'length'},
 {media:'text/plain',body:Buffer.from('ok'),framing:'length',headers:{'content-encoding':'bogus'}},
 );
 async function wire(url,path,input) {
 const address=new URL(url);const before=await(await fetch(url+'/calls')).json();
 const raw=await new Promise((resolve,reject)=>{const chunks=[];const socket=connect(Number(address.port),address.hostname,()=>{
 const headers={'content-type':input.media,'x-correlation-id':'version-body',...input.headers,...(input.framing==='length'?{'content-length':input.body.length}:input.framing==='chunked'?{'transfer-encoding':'chunked'}:{})};
 const head=input.method+' '+path+' HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n'+Object.entries(headers).map(([key,value])=>key+': '+value+'\r\n').join('')+'\r\n';
 const bytes=input.framing==='chunked'?Buffer.from('0\r\n\r\n'):input.body;
 socket.write(Buffer.concat([Buffer.from(head),bytes]));});socket.on('data',chunk=>chunks.push(chunk));socket.on('end',()=>resolve(Buffer.concat(chunks)));socket.on('error',reject);});
 const split=raw.indexOf('\r\n\r\n');const status=Number(raw.toString('latin1').split(' ')[1]);const text=raw.subarray(split+4).toString();const after=await(await fetch(url+'/calls')).json();
 return {status,body:status===200?JSON.parse(text):text,handlers:after.handlers-before.handlers,verifiers:after.verifiers-before.verifiers};
 }
 let exact=0,diagnostics=0,requests=0;
 for(const mode of ['none','warning','error'])for(const optional of [false,true])for(const method of ['POST','DELETE'])for(const original of cases) {
 const input={...original,method};
 const path='/'+mode+'/'+optional;const expected=await wire(ts,path,input);requests++;
 assert.equal(expected.handlers,expected.status===200?1:0);
 for(const surface of ['native','local']) {
 const actual=await wire(rust,'/'+surface+path,input);const label=JSON.stringify({mode,optional,surface,input,expected,actual});
 assert.equal(actual.status,expected.status,label);assert.equal(actual.handlers,expected.handlers,label);assert.equal(actual.verifiers,expected.verifiers,label);
 assert.deepEqual(actual,expected,label);exact++;
 if(expected.status===400&&String(expected.body).startsWith('Version first request')) diagnostics++;
 assert.ok(!JSON.stringify(actual).includes('PRIVATE_VERIFY'));
 }
 }
 const listed=await mcpRpc(rust,'tools/list');const bodyTools=listed.tools.filter(tool=>tool.name!=='calls');assert.equal(bodyTools.length,48);
 for(const tool of bodyTools) {
 const before=await(await fetch(rust+'/calls')).json();
 const result=await mcpRpc(rust,'tools/call',{name:tool.name,arguments:{body:'ok'}});
 assert.equal(result.isError,false,JSON.stringify({tool,result}));assert.deepEqual(JSON.parse(result.content[0].text),{selected:'last',body:'ok'});
 const after=await(await fetch(rust+'/calls')).json();assert.equal(after.verifiers-before.verifiers,1);assert.equal(after.handlers-before.handlers,1);
 }
 console.log('PASS '+bodyTools.length+' real MCP tool calls preserve supplied text bodies with exactly one verifier and handler invocation');
 console.log('PASS '+requests+' actual TypeScript text request cases; '+exact+' exact native/local pairs, '+diagnostics+' exact schema-diagnostic pairs; all-mode status/verify/handler counts and unmatched parser projections match');
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
