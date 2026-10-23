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
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('queryversions','silent'),{mcp:false});
for(const overlap of [false,true])app.get('/query/'+overlap,{name:'Query'+overlap,summary:'Query',versions:{array:{query:{tag:z.array(z.string())},responses:{200:z.unknown()}},scalar:{query:{tag:z.string()},responses:{200:z.unknown()}},...(overlap?{last:{query:{tag:z.union([z.string(),z.array(z.string())])},responses:{200:z.unknown()}}}:{})}},(req,res)=>res.json({version:req.version,query:req.query}));
const server=app.listen(0,'127.0.0.1');server.on('listening',()=>console.log('READY http://127.0.0.1:'+server.address().port));process.on('SIGTERM',()=>server.close(()=>process.exit(0)));
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 const ts=await start(process.execPath,['service.mjs']);
 const queries=['tag=a','tag=a&tag=b','tag=&tag=','tag','tag=%FF&tag=%GG','tag[]=a&tag[]=b','tag=a&unknown=b','tag=a&__proto__=x','tag=a&constructor=x','tag=%E2%82%AC','tag=a+b&tag=a%2Bb','tag=a=b','', 'tag%00=a', '&'.repeat(999)+'tag=yes','&'.repeat(1000)+'tag=yes', 'tag=a&'+'&'.repeat(998)+'tag=b','tag=a&'+'&'.repeat(997)+'tag=b'];
 const oracle=[];
 for(const overlap of [false,true])for(const query of queries){
  const pathname='/query/'+overlap+'?'+query;
  const response=await fetch(ts+pathname);
  const text=await response.text();oracle.push({path:pathname,status:response.status,body:response.status===200?JSON.parse(text):text});
 }
 if(process.env.FORKLAUNCH_VERSION_QUERY_ORACLE_ONLY==='1')console.log(`PASS ${oracle.length} actual Express version query observations`);
 else {
  run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--example','version_query_parity']);
  const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/version_query_parity'),[]);
  let exact=0,errors=0;
  for(const expected of oracle)for(const surface of ['native','local']) {
   const response=await fetch(rust+'/'+surface+expected.path);
   const body=await response.text();
   assert.equal(response.status,expected.status,JSON.stringify({surface,path:expected.path,body}));
   if(expected.status===200){assert.deepEqual(JSON.parse(body),expected.body,JSON.stringify({surface,path:expected.path}));exact++;}
   else {assert.equal(response.status,400);errors++;}
  }
  console.log(`PASS ${exact} exact actual Express/native version query comparisons and ${errors} rejected-version status comparisons (existing diagnostic differences retained)`);
 }
}finally{
 for(const child of children){if(child.exitCode!==null||child.signalCode!==null)continue;await new Promise(resolve=>{child.once('exit',resolve);child.kill('SIGTERM');});}
 rmSync(temporary,{recursive:true,force:true});
}
