import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {createRequire} from 'node:module';
import {readFile,realpath,mkdtemp,writeFile,rm} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {tmpdir} from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc'),ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const temporary=await mkdtemp(path.join(tmpdir(),'fl-mcp-body-'));
let captured;
const wireServer=createServer((request,response)=>{const chunks=[];request.on('data',chunk=>chunks.push(chunk));request.on('end',()=>{captured={body:Buffer.concat(chunks),contentType:request.headers['content-type']??null,contentLength:request.headers['content-length']??null};response.writeHead(200,{'content-type':'application/json'});response.end('{"ok":true}');});});
await new Promise(resolve=>wireServer.listen(0,'127.0.0.1',resolve));
async function fetcher(_url,init){return fetch('http://127.0.0.1:'+wireServer.address().port,init);}
async function load(file,imports={}){const exports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,file),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(name){if(name in imports)return imports[name];throw Error('Unexpected runtime import '+name);},fetch:fetcher,Buffer,URL,URLSearchParams,FormData,Blob});return exports;}
try{
 const {safeStringify}=await load('framework/common/src/safeStringify.ts');
 const {unpackRouters}=await load('framework/core/src/http/router/unpackRouters.ts');
 const discriminate=await load('framework/core/src/http/router/discriminateBody.ts');
 class ZodSchemaValidator{file={};string={};schemify(value){return value;}isInstanceOf(){return false;}}
 class FastMCP{tools=[];addTool(tool){this.tools.push(tool);}}
 const {generateMcpServer}=await load('framework/core/src/http/mcpGenerator/mcpGenerator.ts',{'@forklaunch/common':{safeStringify,isRecord:value=>value!==null&&typeof value==='object'&&!Array.isArray(value),isNever(){throw Error('Unexpected discriminator');}},'@forklaunch/validator/zod':{ZodSchemaValidator,string:{startsWith:()=>({})}},fastmcp:{FastMCP},'../router/discriminateBody':discriminate,'../router/unpackRouters':{unpackRouters}});
 const cases=[];
 for(const body of [{plain:'hello',unicode:'水💬',empty:''},{multi:'a\nb\rc\r\nd'},{'a"\r\nX-Test: value':'payload','percent%':'x'},{}])cases.push({kind:'multipart',arguments:{body}});
 for(const body of [[],null,{flag:false},{value:0},{values:['a']},{file:{base64:'AA=='}}])cases.push({kind:'multipart',arguments:{body}});
 for(const kind of ['file','binary'])for(const body of ['','水💬\u0000bytes',{active:false,ids:[0,null]},[0,255],false,null])cases.push({kind,arguments:{body}});
 for(const body of [{a:'a+b & 水',ids:['a,b','水'],empty:[],filter:{active:false},nil:null,zero:0,flag:false},{}])cases.push({kind:'form',arguments:{body}});
 for(const body of ['plain 水','a\nb\r\nc',''])cases.push({kind:'text',arguments:{body}});
 for(const body of [{active:false,ids:[0,null]},[false,0,null],42,false,null,'','plain','{"flag":"ok"}','水💬','"quoted"\n\\tail'])cases.push({kind:'json',arguments:{body}});
 cases.push({kind:'text',method:'delete',arguments:{body:''}},{kind:'text',method:'delete',arguments:{body:'水'}},{kind:'json',method:'delete',arguments:{body:null}});
 cases.push({kind:'text',method:'options',arguments:{body:''}},{kind:'text',method:'options',arguments:{body:'水'}},{kind:'json',method:'options',arguments:{body:null}});
 for(const method of ['get','post','put','patch','delete','options'])cases.push({kind:'none',method,arguments:{}});
 cases.push({kind:'none',method:'post',arguments:{headers:{'content-type':'application/custom','x-test':'keep'}}},{kind:'none',method:'delete',arguments:{headers:{'Content-Type':'text/custom'}}},{kind:'json',arguments:{body:{ok:true},headers:{'Content-Type':'text/ignored'}}});
 cases.push({kind:'json',contentType:'application/vnd.fixture+json',arguments:{body:{ok:true}}});
 await writeFile(path.join(temporary,'input.json'),JSON.stringify(cases));
 execFileSync('cargo',['test','--quiet','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','rmcp-server,zod-regex,zod-checks,zod-uri','native_mcp_body_transport_conformance'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_MCP_BODY_INPUT:path.join(temporary,'input.json'),FORKLAUNCH_TEST_MCP_BODY_OUTPUT:path.join(temporary,'native.json')},encoding:'utf8',timeout:300000});
 const native=JSON.parse(await readFile(path.join(temporary,'native.json'),'utf8'));
 const typed={json:'json',text:'text',file:'file',binary:'file',multipart:'multipartForm',form:'urlEncodedForm'};
 for(let index=0;index<cases.length;index++){
  const input=cases[index],bodySchema=input.kind==='none'?undefined:{[typed[input.kind]]:{},...(input.contentType?{contentType:input.contentType}:{})};
  const app={basePath:'/',routers:[],routes:[{method:input.method??'post',path:'/body',contractDetails:{name:'Body',summary:'Body',body:bodySchema,responses:{200:{json:{}}},options:{mcp:true}}}]};
  const server=generateMcpServer(new ZodSchemaValidator(),'http','127.0.0.1',9000,'1.0.0',app);captured=undefined;
  const originalArguments=JSON.stringify(input.arguments);
  let error;try{await server.tools[0].execute(input.arguments);}catch(cause){error=cause;}
  assert.equal(JSON.stringify(input.arguments),originalArguments,'generator must not mutate caller headers/arguments');
  if(native[index].error){assert.ok(error,`TS accepted rejected multipart case ${index}`);assert.equal(captured,undefined);continue;}
  assert.equal(error,undefined,`TS body execution failed ${index}: ${error}`);assert.ok(captured);
  const nativeBody=Buffer.from(native[index].body,'base64');
  assert.equal(captured.contentLength,['get','delete','options'].includes(input.method)&&captured.body.length===0?null:String(captured.body.length),'actual fetch encoded-body framing');
  assert.equal(native[index].contentLength,['get','delete','options'].includes(input.method)&&nativeBody.length===0?null:String(nativeBody.length),'native trusted encoded-body framing');
  if(input.kind==='multipart'){
   const parse=async(body,contentType)=>[...await new Response(body,{headers:{'content-type':contentType}}).formData()];
   assert.deepEqual(await parse(nativeBody,native[index].contentType),await parse(captured.body,captured.contentType),`Multipart fields ${index}`);
   const normalize=(body,mime)=>body.toString().replaceAll(mime.split('boundary=')[1],'BOUNDARY').replace(/\r\n$/,'');
   assert.equal(normalize(nativeBody,native[index].contentType),normalize(captured.body,captured.contentType),`Multipart framing ${index}`);
  }else{assert.equal(native[index].contentType,captured.contentType,`Content type ${index}`);assert.deepEqual(nativeBody,captured.body,`Body bytes ${index}`);}
  if(input.kind==='json')assert.deepEqual(JSON.parse(captured.body.toString()),input.arguments.body,`Typed JSON value ${index}`);
 }
 let optionalBoundaries=0;
 for(const kind of ['text','json'])for(const args of [{},{body:null},{body:''}]) {
 const app={basePath:'/',routers:[],routes:[{method:'delete',path:'/body',contractDetails:{name:'Optional',summary:'Optional',body:{[typed[kind]]:{}},responses:{200:{json:{}}},options:{mcp:true}}}]};
 const server=generateMcpServer(new ZodSchemaValidator(),'http','127.0.0.1',9000,'1.0.0',app);captured=undefined;await server.tools[0].execute(args);assert.ok(captured);
 if(args.body===undefined||(kind==='text'&&args.body===null)){assert.equal(captured.contentLength,null);assert.equal(captured.body.length,0);}else{
 const expected=kind==='json'?JSON.stringify(args.body):args.body;assert.equal(captured.body.toString(),expected);assert.equal(captured.contentLength,expected===''?null:String(Buffer.byteLength(expected)));}
 optionalBoundaries++;
 }
 console.log(`PASS ${optionalBoundaries} actual DELETE omitted/null/empty body characterization cases; native Missing body and text-null rejection remain explicit admission differences`);
 console.log(`PASS ${cases.length} actual TS MCP generator/discriminator and native body cases: multipart framing/Unicode/newlines/names, invalid multipart shapes, file/binary JSON bytes, form JSON composites, text/JSON and custom media types`);
}finally{wireServer.closeAllConnections();await new Promise(resolve=>wireServer.close(resolve));await rm(temporary,{recursive:true,force:true});}
