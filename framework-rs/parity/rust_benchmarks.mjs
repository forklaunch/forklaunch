import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,rmSync,symlinkSync} from 'node:fs';
import {tmpdir,cpus,platform,arch,release,totalmem} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const profile=process.argv.includes('--debug')?'debug':'release';
const iterations=Number(process.env.FORKLAUNCH_BENCH_ITERATIONS??1000);
const warmup=Number(process.env.FORKLAUNCH_BENCH_WARMUP??100);
for(const [name,value] of Object.entries({iterations,warmup}))assert.ok(Number.isInteger(value)&&value>0&&value<=1000000,`invalid ${name}`);
if(process.argv.includes('--require-infra'))assert.ok(process.env.FORKLAUNCH_BENCH_REDIS_URL&&process.env.FORKLAUNCH_BENCH_S3==='1','both Redis and S3 must be enabled');
const temporary=mkdtempSync(join(tmpdir(),'fl-runtime-benchmarks-'));
const measurements=[];let child;
function run(command,args,cwd=temporary){
 const result=spawnSync(command,args,{cwd,env:{...process.env,CARGO_INCREMENTAL:'0'},encoding:'utf8',timeout:900000,maxBuffer:16*1024*1024});
 assert.equal(result.status,0,`${result.error??''}\n${result.stdout}\n${result.stderr}`);return result.stdout.trim();
}
try{
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-testing','--features','benchmarks','--example','runtime_benchmarks',...(profile==='release'?['--release']:[])]);
 const binary=join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),profile,'examples/runtime_benchmarks');
 let diagnostics='';
 const ready=await new Promise((resolve,reject)=>{
  child=spawn(binary,[join(temporary,'sdk.ts')],{cwd:temporary,env:{...process.env,FORKLAUNCH_BENCH_ITERATIONS:String(iterations),FORKLAUNCH_BENCH_WARMUP:String(warmup)},stdio:['pipe','pipe','pipe']});
  const timer=setTimeout(()=>reject(new Error(`benchmark readiness timed out: ${diagnostics}`)),600000);
  let buffer='';child.stdout.on('data',chunk=>{
   buffer+=chunk;let end;
   while((end=buffer.indexOf('\n'))>=0){
    const line=buffer.slice(0,end);buffer=buffer.slice(end+1);
    try { const value=JSON.parse(line);if(value.ready){clearTimeout(timer);resolve(value);}else measurements.push(value); }
    catch(error){clearTimeout(timer);reject(error);}
   }
  });
  child.stderr.on('data',chunk=>{diagnostics=(diagnostics+chunk).slice(-65536);});
  child.on('error',error=>{clearTimeout(timer);reject(error);});
  child.on('exit',code=>{clearTimeout(timer);reject(new Error(`native benchmark exited ${code}: ${diagnostics}`));});
 });
 assert.equal(ready.profile,profile);
 writeFileSync(join(temporary,'package.json'),'{"type":"module"}');
 // The generated SDK imports its validator (ajv); resolve it from the TypeScript toolchain install.
 if(process.env.TSC_BIN)symlinkSync(dirname(dirname(resolve(process.env.TSC_BIN))),join(temporary,'node_modules'),'dir');
 run(process.env.TSC_BIN??'tsc',['sdk.ts','--target','ES2022','--module','ES2022','--moduleResolution','node','--lib','ES2022,DOM','--strict','--outDir','compiled']);
 const {createForkLaunchSdk}=await import(pathToFileURL(join(temporary,'compiled/sdk.js')));
 const sdk=createForkLaunchSdk({baseUrl:ready.ready});const samples=[];
 for(let index=0;index<iterations+warmup;index++){
  const start=process.hrtime.bigint();
  const result=await sdk.presence.presence.readPresence({params:{index:'one'},signal:AbortSignal.timeout(10000)});
  const elapsed=Number(process.hrtime.bigint()-start);
  assert.equal(result.status,200);assert.deepEqual(result.response,{id:'x'.repeat(1024)});
  if(index>=warmup)samples.push(elapsed);
 }
 samples.sort((a,b)=>a-b);
 measurements.push({benchmark:'generated_typescript_sdk_http',samples:iterations,warmup,total_ns:samples.reduce((a,b)=>a+b,0),min_ns:samples[0],p50_ns:samples[Math.floor((iterations-1)/2)],p95_ns:samples[Math.ceil(iterations*.95)-1],max_ns:samples.at(-1)});
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>{child.kill('SIGKILL');reject(new Error('native benchmark shutdown timed out'));},10000);
  child.once('exit',code=>{clearTimeout(timer);code===0?resolve():reject(new Error(`native exit ${code}: ${diagnostics}`));});child.stdin.end('\n');
 });
 assert.equal(measurements.length,6);
 if(process.argv.includes('--require-infra'))assert.ok(measurements.every(value=>!value.skipped));
 const report={schema_version:1,recorded_at:new Date().toISOString(),revision:run('git',['rev-parse','HEAD'],root),working_tree_dirty:run('git',['status','--porcelain'],root)!=='',profile,concurrency:1,payload_bytes:ready.payload_bytes,rustc:run('rustc',['--version']),node:process.version,host:{platform:platform(),arch:arch(),release:release(),cpu:cpus()[0]?.model,logical_cpus:cpus().length,memory_bytes:totalmem()},measurements};
 const output=JSON.stringify(report,null,2)+'\n';
 if(process.env.FORKLAUNCH_BENCH_OUTPUT)writeFileSync(process.env.FORKLAUNCH_BENCH_OUTPUT,output);
 process.stdout.write(output);
}finally{
 if(child&&child.exitCode===null&&child.signalCode===null){
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),3000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
