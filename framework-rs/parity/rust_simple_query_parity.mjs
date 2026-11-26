import assert from 'node:assert/strict';
import {parse as parseQuery} from 'node:querystring';
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
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('query','silent'),{mcp:false});

app.get('/:mode',{name:'Query',summary:'Query',options:{requestValidation:'none',responseValidation:'none'},responses:{200:z.unknown()}},(req,res)=>res.json(req.query));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 const ts=await start(process.execPath,['service.mjs']);
 const cases=['','a=1&a=2','a[b]=x&a[]=y','__proto__=x&constructor=y&toString=z','__proto__[polluted]=yes&a[constructor]=x','a=%ff+%20&b=%E6%B0%B4&c=%2','a=%E0%A4%A','a=1&&b=2&','&=x&&=y','a[25]=x&a[0]=y','a[b][c][d][e][f][g]=x','a=1=2','a.b=x','a[]=1&a[]=2',Array.from({length:1002},(_,i)=>'k'+i+'=x').join('&')];
 cases.push(null,'&'.repeat(1000)+'last=x','&'.repeat(999)+'last=x','='.repeat(20),'a=+%2B+','x=%00','x=%ED%A0%80','x=%F0%80%80%80');
 // Exhaust all single and double encoded byte combinations against Node's decoder.
 const exhaustive=[...cases];
 for(let first=0;first<256;first++) {
  const a='%'+first.toString(16).padStart(2,'0');exhaustive.push('x='+a);
  for(let second=0;second<256;second++)exhaustive.push('x='+a+'%'+second.toString(16).padStart(2,'0'));
 }
 for(const tail of ['%','%0','%GG','%E2%82','%F0%9F%92','%ED%A0%80'])for(const head of ['','a','%','%FF','%E2','+','%25'])exhaustive.push('x='+head+tail);
 // Member order: array-index names ascending (up to 2**32-2), then first occurrence.
 exhaustive.push('b=1&a=2&10=3&2=4&b=5&01=6&4294967295=x&4294967294=y&-1=z');
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','simple_query_parity']);
 const binary=join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/simple_query_parity');
 const result=spawnSync(binary,[],{input:exhaustive.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',maxBuffer:32*1024*1024});
 assert.equal(result.status,0,result.stderr);const lines=result.stdout.trim().split('\n').map(value=>JSON.parse(value));const outputs=lines.map(line=>line.fields);assert.equal(outputs.length,exhaustive.length);
 exhaustive.forEach((raw,index)=>{assert.deepEqual(outputs[index],JSON.parse(JSON.stringify(parseQuery(raw??''))),JSON.stringify(raw));assert.deepEqual(lines[index].orderedKeys,Object.keys(parseQuery(raw??'')),`order ${JSON.stringify(raw)}`);});
 let wire=0;
 for(const raw of cases){const response=await fetch(ts+'/default'+(raw===null?'':'?'+raw));assert.equal(response.status,200);const value=await response.json();assert.deepEqual(value,outputs[wire],JSON.stringify(raw));wire++;}
 console.log(`PASS ${exhaustive.length} actual Node/native simple-query comparisons (values and member order) and ${wire} actual default ForkLaunch Express wire observations`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
