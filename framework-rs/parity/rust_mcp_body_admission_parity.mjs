import assert from 'node:assert/strict';
import {mkdtempSync,cpSync,mkdirSync,symlinkSync,writeFileSync,readFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync,spawn} from 'node:child_process';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES');
const temporary=mkdtempSync(join(tmpdir(),'fl-mcp-admission-'));
let child;
function run(command,args,cwd=temporary){const result=spawnSync(command,args,{cwd,env:{...process.env,CARGO_INCREMENTAL:'0'},encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});assert.equal(result.status,0,result.stdout+'\n'+result.stderr);}
try {
 symlinkSync(join(dependencies,'node_modules'),join(temporary,'node_modules'));
 for(const name of ['core','common','validator','express']) {const dest=join(temporary,'source',name);mkdirSync(dest,{recursive:true});cpSync(join(root,'framework',name,'src'),join(dest,'src'),{recursive:true});if(name!=='core')cpSync(join(root,'framework',name,'index.ts'),join(dest,'index.ts'));writeFileSync(join(dest,'package.json'),JSON.stringify({type:'module'}));}
 cpSync(join(root,'framework/validator/shims'),join(temporary,'source/validator/shims'),{recursive:true});
 writeFileSync(join(temporary,'package.json'),'{"type":"module"}');
 writeFileSync(join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{baseUrl:'.',paths:{'@forklaunch/core/*':['source/core/src/*/index.ts'],'@forklaunch/express':['source/express/index.ts'],'@forklaunch/common':['source/common/index.ts'],'@forklaunch/validator':['source/validator/index.ts'],'@forklaunch/validator/*':['source/validator/src/*/index.ts']},target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true}}));
 writeFileSync(join(temporary,'audit.ts'),readFileSync(join(root,'framework-rs/parity/fixtures/mcp-body-admission.ts'),'utf8'));
 run(join(dependencies,'node_modules/.bin/esbuild'),['audit.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=audit.mjs']);run(process.execPath,['audit.mjs']);
 const expected=JSON.parse(readFileSync(join(temporary,'observations.json'),'utf8'));
 writeFileSync(join(temporary,'omission.ts'),readFileSync(join(root,'framework-rs/parity/fixtures/mcp-body-omission.ts'),'utf8'));
 run(join(dependencies,'node_modules/.bin/esbuild'),['omission.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=omission.mjs']);run(process.execPath,['omission.mjs']);
 console.log('PASS 140 actual registered FastMCP/Express omission, framing and downstream schema assertions');
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','rmcp-server','--example','mcp_body_admission'],root);
 child=spawn(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/mcp_body_admission'),[],{stdio:['ignore','pipe','pipe']});
 const base=await new Promise((resolve,reject)=>{let output='';const timeout=setTimeout(()=>reject(Error(output)),30000);for(const stream of [child.stdout,child.stderr])stream.on('data',chunk=>{output+=chunk;const match=output.match(/READY (http:\/\/127\.0\.0\.1:\d+)/);if(match){clearTimeout(timeout);resolve(match[1]);}});child.on('exit',()=>{clearTimeout(timeout);reject(Error(output));});});
 async function rpc(method,params){const response=await fetch(base+'/mcp',{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});assert.equal(response.status,200);return response.json();}
 const listed=await rpc('tools/list',{});assert.equal(listed.result.tools.length,20);
 for(const tool of listed.result.tools){const reference=expected.tools.find(item=>item.name===tool.name);assert.ok(reference,JSON.stringify({name:tool.name,expected:expected.tools.map(tool=>tool.name)}));assert.equal(tool.inputSchema.required?.includes('body')??false,reference.inputSchema.required?.includes('body')??false);assert.deepEqual(tool.inputSchema.properties.body.default,reference.inputSchema.properties.body.default);}
 let accepted=0,rejected=0,envelopeBoundaries=0;
 for(const row of expected.rows){if(row.input==='null_arguments'){assert.equal(row.result.error?.code,-32603);assert.equal(row.wire,null);envelopeBoundaries++;continue;}const result=await rpc('tools/call',{name:row.name,arguments:row.args});if(row.result.error){assert.equal(result.error?.code,-32602,JSON.stringify({row,result}));rejected++;}else{assert.ok(!result.error,JSON.stringify({row,result}));assert.ok(!result.result.isError);assert.deepEqual(JSON.parse(result.result.content[0].text),row.wire,JSON.stringify({row,result}));accepted++;}}
 assert.equal(accepted+rejected,120);assert.equal(envelopeBoundaries,20);console.log('PASS 20 actual TS whole-arguments-null protocol rejections; native envelope null admission remains outside body-profile scope');console.log(`PASS ${accepted} real registered FastMCP/native admitted wire comparisons, ${rejected} rejected admission comparisons; 20 tool requiredness/default declarations; diagnostic text deliberately not equated`);
} finally { if(child){child.kill('SIGTERM');await new Promise(resolve=>child.exitCode!==null?resolve():child.once('exit',resolve));}rmSync(temporary,{recursive:true,force:true});}
