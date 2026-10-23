import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,symlinkSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES');
const expected=JSON.parse(readFileSync(join(root,'framework-rs/parity/fixtures/canonical-iam/package.json'))).dependencies;
for(const name of ['fastmcp','zod','@modelcontextprotocol/sdk','esbuild'])assert.equal(JSON.parse(readFileSync(join(dependencies,'node_modules',name,'package.json'))).version,expected[name]);
const temporary=mkdtempSync(join(tmpdir(),'fl-mcp-transport-'));
function run(command,args,env=process.env){const result=spawnSync(command,args,{cwd:temporary,env,encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});assert.equal(result.status,0,`${result.error??''}\n${result.stdout}\n${result.stderr}`);}
try{
 symlinkSync(join(dependencies,'node_modules'),join(temporary,'node_modules'));
 for(const name of ['core','common','validator']){
  const dest=join(temporary,'source',name);mkdirSync(dest,{recursive:true});
  cpSync(join(root,'framework',name,'src'),join(dest,'src'),{recursive:true});
  if(name!=='core')cpSync(join(root,'framework',name,'index.ts'),join(dest,'index.ts'));
  writeFileSync(join(dest,'package.json'),JSON.stringify({type:'module'}));
 }
 cpSync(join(root,'framework/validator/shims'),join(temporary,'source/validator/shims'),{recursive:true});
 writeFileSync(join(temporary,'package.json'),'{"private":true,"type":"module"}');
 writeFileSync(join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{baseUrl:'.',paths:{
  '@fixture/mcp':['source/core/src/http/mcpGenerator/mcpGenerator.ts'],
  '@forklaunch/common':['source/common/index.ts'],'@forklaunch/validator':['source/validator/index.ts'],
  '@forklaunch/validator/*':['source/validator/src/*/index.ts']},target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true}}));
 const cases=[];
 for(const body of [{plain:'hello',unicode:'水💬',empty:''},{multi:'a\nb\rc\r\nd'},{'a"\r\nX-Test: value':'payload','percent%':'x'},{}])cases.push({kind:'multipart',arguments:{body}});
 for(const body of [[],null,{flag:false},{value:0},{values:['a']},{file:{base64:'AA=='}}])cases.push({kind:'multipart',arguments:{body}});
 for(const kind of ['file','binary'])for(const body of ['','水💬\u0000bytes',{active:false,ids:[0,null]},[0,255],false,null])cases.push({kind,arguments:{body}});
 for(const body of [{a:'a+b & 水',ids:['a,b','水'],empty:[],filter:{active:false},nil:null,zero:0,flag:false},{}])cases.push({kind:'form',arguments:{body}});
 for(const body of ['plain 水','a\nb\r\nc',''])cases.push({kind:'text',arguments:{body}});
 for(const body of [{active:false,ids:[0,null]},[false,0,null],42,false,null,'','scalar 水','{"flag":"ok"}','"quoted"\n\\tail'])cases.push({kind:'json',arguments:{body}});
 cases.push({kind:'json',contentType:'application/vnd.fixture+json',arguments:{contentType:'application/vnd.fixture+json',body:{ok:true}}});
 writeFileSync(join(temporary,'input.json'),JSON.stringify(cases));
 run('cargo',['test','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','rmcp-server,zod-regex,zod-checks,zod-uri','native_mcp_body_transport_conformance'],{...process.env,CARGO_INCREMENTAL:'0',FORKLAUNCH_TEST_MCP_BODY_INPUT:join(temporary,'input.json'),FORKLAUNCH_TEST_MCP_BODY_OUTPUT:join(temporary,'native.json')});
 cpSync(join(root,'framework-rs/parity/fixtures/mcp_transport.ts'),join(temporary,'transport.ts'));
 run(join(dependencies,'node_modules/.bin/esbuild'),['transport.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=transport.mjs']);
 run(process.execPath,['transport.mjs']);
 const native=JSON.parse(readFileSync(join(temporary,'native.json'))),actual=JSON.parse(readFileSync(join(temporary,'actual.json')));
 assert.equal(actual.length,cases.length);
 for(let index=0;index<cases.length;index++){
  assert.equal(actual[index].error,Boolean(native[index].error),`rejection ${index}: ${JSON.stringify(actual[index])}`);
  if(actual[index].error){assert.equal(actual[index].captured,false);continue;}
  const left=Buffer.from(native[index].body,'base64'),right=Buffer.from(actual[index].body,'base64');
  if(cases[index].kind==='multipart'){
   const parse=async(body,mime)=>[...await new Response(body,{headers:{'content-type':mime}}).formData()];
   assert.deepEqual(await parse(left,native[index].contentType),await parse(right,actual[index].contentType));
  }else{assert.equal(native[index].contentType,actual[index].contentType);assert.deepEqual(left,right,`wire bytes ${index}`);}
  if(cases[index].kind==='json')assert.deepEqual(JSON.parse(right.toString()),cases[index].arguments.body,`Typed JSON value ${index}`);
 }
 console.log(`PASS ${cases.length} actual FastMCP + repository Zod/generator MCP calls against live HTTP, compared with native wire encoding; named-version POST/GET and mismatched-input regressions pass`);
}finally{rmSync(temporary,{recursive:true,force:true});}
