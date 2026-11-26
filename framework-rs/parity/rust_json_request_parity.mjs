import {connect} from 'node:net';
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
const temporary=mkdtempSync(join(tmpdir(),'fl-json-parser-'));
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
let handlerCalls=0,verifyCalls=0,revivedNumbers=[];
function shape(value){if(typeof value==='number')return '<number>';if(Array.isArray(value))return value.map(shape);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,shape(value)]));return value;}
function numbers(value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return [b.toString('hex')];}if(Array.isArray(value))return value.flatMap(numbers);if(value&&typeof value==='object')return Object.keys(value).sort().flatMap(key=>numbers(value[key]));return [];}
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('parser','silent'),{mcp:false,json:{limit:128,strict:process.argv[2]!=='false',...(process.argv[3]==='true'?{reviver(key,value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);revivedNumbers.push(b.toString('hex'));}return value;}}:{}),verify(req){revivedNumbers=[];verifyCalls++;if(req.headers['x-deny']==='yes')throw new Error('PRIVATE_VERIFY');}}});
for(const mode of ['none','warning','error'])for(const auth of [false,true])app.post('/parse/'+mode+'/'+auth,{name:'Parse'+mode+auth,summary:'Parse',body:{value:z.unknown().optional()},...(auth?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),options:{requestValidation:mode,responseValidation:'none'},responses:{200:z.unknown()}},(req,res)=>{handlerCalls++;res.json({kind:req.body===undefined?'undefined':req.body===null?'null':Array.isArray(req.body)?'array':typeof req.body,value:shape(req.body),raw:req._rawBody?.toString('hex'),numbers:numbers(req.body),...(process.argv[3]==='true'?{revivedNumbers}:{})});});
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:{handlerCalls:z.number(),verifyCalls:z.number()}}},(_req,res)=>res.json({handlerCalls,verifyCalls}));
const server=app.listen(0,'127.0.0.1');
server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));
process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','json_request_parity']);
 // Compare numeric leaves as IEEE bits independently of response serializer formatting (-0, exponents).
 const cases=[
 ['absent',{},Buffer.alloc(0),'none'],['empty',{'content-type':'application/json'},Buffer.alloc(0),'length'],['chunkedempty',{'content-type':'application/json'},Buffer.alloc(0),'chunked'],
 ['missingmedia',{},Buffer.from('{'),'length'],['wrongmedia',{'content-type':'text/plain'},Buffer.from('{'),'length'],['suffixmedia',{'content-type':'application/problem+json'},Buffer.from('{'),'length'],
 ['malformed',{},Buffer.from('{')],['null',{},Buffer.from('null')],['string',{},Buffer.from('"value"')],['array',{},Buffer.from('[]')],['object',{},Buffer.from('{"value":1}')],['whitespace',{},Buffer.from('   ')],['bom',{},Buffer.from('\ufeff{"value":1}')],
 ['limit',{},Buffer.from('{"value":"'+'x'.repeat(140)+'"}')],['charset',{'content-type':'application/json;charset=iso-8859-1'},Buffer.from('{}')],['utf16',{'content-type':'application/json;charset=utf-16le'},Buffer.from('{"value":1}','utf16le')],['deny',{'x-deny':'yes'},Buffer.from('{}')],['gzip',{'content-encoding':'gzip'},gzipSync(Buffer.from('{"value":1}'))],['badgzip',{'content-encoding':'gzip'},Buffer.from('invalid')],['encoding',{'content-encoding':'unknown'},Buffer.from('{}')],['nested',{},Buffer.from('{"value":{"z":[9007199254740993,-0],"2":18446744073709551615,"a":1e300}}')],['rounding',{},Buffer.from('{"value":9007199254740993}')],['infinity',{},Buffer.from('{"value":1e400}')],['surrogate',{},Buffer.from('{"value":"\\ud800"}')],...['12.99','0.1','1.00000000000000011102230246251565404236316680908203125','-0','1e-320','5e-324','2.4703282292062327e-324','2.4703282292062328e-324','1e2','9007199254740991','-9007199254740991','0.99999999999999994','9007199254740992','9007199254740993','9007199254740995','-9007199254740993','9223372036854775807','-9223372036854775808','18446744073709551615','18446744073709551616','1e300','-1e300','1.7976931348623157e308'].map((value,index)=>['number'+index,{},Buffer.from('{"value":'+value+'}')])
 ];
 let comparisons=0, unsupported=0;
 for(const strict of [true,false])for(const reviver of [false,true]){
  const ts=await start(process.execPath,['service.mjs',String(strict),String(reviver)]);
  const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/json_request_parity'),[String(strict),String(reviver)]);
  for(const [name,headers,body,framing='length'] of cases)for(const [mode,auth] of [['none',false],['warning',false],['none',true]]){
   const results=[];
   for(const url of [ts,rust]){
   const address=new URL(url);const before=await(await fetch(url+'/calls')).json();
   const raw=await new Promise((resolve,reject)=>{const chunks=[];const socket=connect(Number(address.port),address.hostname,()=>{
    const media=['absent','missingmedia','wrongmedia','suffixmedia'].includes(name)?{}:{'content-type':'application/json'};
    const fields={...media,...headers,'x-correlation-id':'json-oracle',...(framing==='length'?{'content-length':body.length}:framing==='chunked'?{'transfer-encoding':'chunked'}:{})};
    const head='POST /parse/'+mode+'/'+auth+' HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n'+Object.entries(fields).map(([key,value])=>key+': '+value+'\r\n').join('')+'\r\n';
    const bytes=framing==='chunked'?Buffer.from(body.length?body.length.toString(16)+'\r\n'+body.toString()+'\r\n0\r\n\r\n':'0\r\n\r\n'):body;
    socket.write(Buffer.concat([Buffer.from(head),bytes]));
   });socket.on('data',chunk=>chunks.push(chunk));socket.on('end',()=>resolve(Buffer.concat(chunks)));socket.on('error',reject);});
   const split=raw.indexOf('\r\n\r\n');const responseBody=raw.subarray(split+4).toString();const status=Number(raw.toString('latin1').split(' ')[1]);const after=await(await fetch(url+'/calls')).json();
   results.push({status,body:status===200?JSON.parse(responseBody):status===401?'AUTH_REJECTION':responseBody,handlerCalls:after.handlerCalls-before.handlerCalls,verifyCalls:after.verifyCalls-before.verifyCalls});
   }
   if(['infinity','surrogate'].includes(name)){
    assert.equal(results[0].status,auth?401:200);assert.equal(results[0].handlerCalls,auth?0:1);assert.equal(results[0].verifyCalls,1);
    assert.equal(results[1].status,400);assert.equal(results[1].handlerCalls,0);assert.equal(results[1].verifyCalls,1);unsupported++;
   }else {assert.deepEqual(results[1],results[0],JSON.stringify({strict,reviver,name,mode,auth,results}));comparisons++;}
  }
 }
 console.log(`PASS ${comparisons} actual TypeScript/native JSON parser comparisons (${comparisons*2} observations); ${unsupported} explicit unsupported numeric/Unicode domain checks`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
