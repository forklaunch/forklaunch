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
const temporary=mkdtempSync(join(tmpdir(),'fl-composition-runtime-'));
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
import {createServer} from 'node:http';
import {generateMcpServer} from './source/core/src/http/mcpGenerator/mcpGenerator';
import {SchemaValidator,number,boolean,enum_,record,uri} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
const event=z.discriminatedUnion('kind',[z.object({kind:z.enum(['create','update']),label:z.string().trim()}),z.object({kind:z.nativeEnum({'0':'A',A:0}),count:number})]).optional();
let handlerCalls=0;
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('request-transform','silent'),{mcp:false,json:{strict:false}});
for(const mode of ['none','warning','error'])for(const profile of ['public','protected','default','remove'])app.post('/transform/'+mode+'/'+profile,{name:'Transform'+mode+profile,summary:'Transform',body:profile==='remove'?{json:z.unknown().transform(()=>undefined)}:profile==='default'?{json:z.object({amount:number,boundedItems:z.array(z.string().trim()).min(4,'replaced').min(1.5,'array minimum').max(2.5,'').length(NaN,'inert').optional(),floatText:z.string().min(1.5).max(2.5).optional(),nanText:z.string().length(NaN).optional(),caseText:z.string().toUpperCase().trim().max(2).optional(),lowerText:z.string().toLowerCase().optional(),event,label:z.string().default('fallback'),kind:enum_({create:'create',update:'update'}).optional(),choice:z.union([z.string().trim().min(3),number.pipe(z.number().int()),z.object({nested:z.union([z.string(),z.array(boolean)])})]).optional(),labels:record(z.string().trim(),number).optional(),settings:z.object({enabled:boolean}).strict().optional(),items:z.array(z.object({enabled:boolean})).optional(),cleared:z.null().optional(),omitted:z.undefined(),voided:z.void(),forbidden:z.never().optional(),nothing:z.union([z.void(),z.null(),z.undefined()]),bag:z.object({label:z.string()}).passthrough().optional(),scores:z.object({label:z.string()}).strict().catchall(number).optional(),pair:z.tuple([z.string().trim(),number]).optional(),trail:z.tuple([z.string().trim(),number]).rest(boolean).optional(),uri:uri.optional(),joined:z.object({left:z.string().trim()}).and(z.object({right:number})).optional(),conflict:z.string().trim().and(z.string()).optional(),formats:z.object({time:z.string().trim().time({precision:3}),jwt:z.string().jwt({alg:'HS256'}),ip:z.string().ip({version:'v6'}),emoji:z.string().emoji()}).optional()}).default({amount:7,label:'fallback'})}:{amount:number,boundedItems:z.array(z.string().trim()).min(4,'replaced').min(1.5,'array minimum').max(2.5,'').length(NaN,'inert').optional(),floatText:z.string().min(1.5).max(2.5).optional(),nanText:z.string().length(NaN).optional(),caseText:z.string().toUpperCase().trim().max(2).optional(),lowerText:z.string().toLowerCase().optional(),event,label:z.string().default('fallback'),kind:enum_({create:'create',update:'update'}).optional(),choice:z.union([z.string().trim().min(3),number.pipe(z.number().int()),z.object({nested:z.union([z.string(),z.array(boolean)])})]).optional(),labels:record(z.string().trim(),number).optional(),settings:z.object({enabled:boolean}).strict().optional(),items:z.array(z.object({enabled:boolean})).optional(),cleared:z.null().optional(),omitted:z.undefined(),voided:z.void(),forbidden:z.never().optional(),nothing:z.union([z.void(),z.null(),z.undefined()]),bag:z.object({label:z.string()}).passthrough().optional(),scores:z.object({label:z.string()}).strict().catchall(number).optional(),pair:z.tuple([z.string().trim(),number]).optional(),trail:z.tuple([z.string().trim(),number]).rest(boolean).optional(),uri:uri.optional(),joined:z.object({left:z.string().trim()}).and(z.object({right:number})).optional(),conflict:z.string().trim().and(z.string()).optional(),formats:z.object({time:z.string().trim().time({precision:3}),jwt:z.string().jwt({alg:'HS256'}),ip:z.string().ip({version:'v6'}),emoji:z.string().emoji()}).optional()},...(profile==='protected'?{access:'authenticated',auth:{jwt:{signatureKey:'test-only'}}}:{}),options:{requestValidation:mode,responseValidation:'none'},responses:{200:z.unknown()}},(req,res)=>{handlerCalls++;res.json({hasBody:req.body!==undefined,body:req.body,raw:req._rawBody?.toString()});});
app.get('/calls',{name:'Calls',summary:'Calls',responses:{200:{handlerCalls:z.number()}}},(_req,res)=>res.json({handlerCalls}));
const server=app.listen(0,'127.0.0.1');await new Promise(resolve=>server.on('listening',resolve));
const port=server.address().port;
const free=createServer();await new Promise(resolve=>free.listen(0,'127.0.0.1',resolve));const mcpPort=free.address().port;await new Promise(resolve=>free.close(resolve));
// Transport admission is intentionally permissive: downstream HTTP owns schema validation.
const routes=[];for(const mode of ['none','warning','error'])routes.push({method:'post',path:'/transform/'+mode+'/public',contractDetails:{name:'Transform'+mode+'public',summary:'Transform',body:{json:z.unknown()},responses:{200:{json:z.unknown()}},options:{mcp:true}}});
const mcp=generateMcpServer(SchemaValidator(),'http','127.0.0.1',port,'1.0.0',{basePath:'/',routers:[],routes});
await mcp.start({transportType:'httpStream',httpStream:{host:'127.0.0.1',port:mcpPort,endpoint:'/mcp',stateless:true,enableJsonResponse:true}});
app.get('/mcp-origin',{name:'McpOrigin',summary:'Origin',responses:{200:z.unknown()}},(_req,res)=>res.json({url:'http://127.0.0.1:'+mcpPort}));
console.log('READY http://127.0.0.1:'+port);process.on('SIGTERM',async()=>{await mcp.stop();server.close(()=>process.exit(0));});
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);

 if(process.env.FORKLAUNCH_COMPOSITION_TS_ONLY==='1') {
  const ts=await start(process.execPath,['service.mjs']);
  let count=0;
  for(const mode of ['none','warning','error'])for(const body of [{amount:'2',items:[{enabled:'true',drop:1}],extra:true},{amount:'bad',items:[{enabled:[]}],extra:true}]) {
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,body.amount==='bad'&&mode==='error'?400:200);
   if(response.status===200){const report=await response.json();assert.deepEqual(report.body,body.amount==='bad'?body:{amount:2,label:'fallback',items:[{enabled:true}]});}count++;
  }
  for(const mode of ['none','warning','error'])for(const choice of [' good ',{nested:false}]) {
   const body={amount:'2',choice,extra:true};const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   const invalid=typeof choice==='object';assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',choice:'good'});
   else assert.ok((await response.text()).includes('Union Schema Variant'));count++;
  }
  for(const mode of ['none','warning','error'])for(const kind of ['create','unknown']) {
   const body={amount:'2',kind,extra:true};const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   const invalid=kind==='unknown';assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',kind});
   else assert.ok((await response.text()).includes('Invalid literal value'));count++;
  }
  for(const mode of ['none','warning','error'])for(const settings of [{enabled:'true'},{enabled:'true',z:1,a:2}]) {
   const body={amount:'2',settings};const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   const invalid='z' in settings;assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',settings:{enabled:true}});
   else assert.ok((await response.text()).includes("Unrecognized key(s) in object: 'z', 'a'"));count++;
  }
  for(const mode of ['none','warning','error'])for(const labels of [{' a ':'1',a:'2'},{z:'bad',a:'bad'}]) {
   const body={amount:'2',labels};const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   const invalid='z' in labels;assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',labels:{a:2}});
   else assert.ok((await response.text()).includes('labels'));count++;
  }
  for(const mode of ['none','warning','error'])for(const field of ['cleared','omitted','voided','forbidden','nothing']) {
   const body={amount:'2',[field]:null};const invalid=['omitted','voided','forbidden'].includes(field);
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',[field]:null});
   else assert.ok((await response.text()).includes(field));count++;
  }
  for(const mode of ['none','warning','error'])for(const body of [{amount:'2',bag:{label:'ok',extra:'kept'},scores:{label:'ok',z:'2'}},{amount:'2',scores:{label:0,z:'bad',a:'bad'}}]) {
   const invalid=body.scores.label===0;
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',bag:{label:'ok',extra:'kept'},scores:{label:'ok',z:2}});
   else assert.ok((await response.text()).includes('scores'));count++;
  }
  for(const mode of ['none','warning','error'])for(const formats of [{time:' 12:34:56.123 ',jwt:'eyJhbGciOiJIUzI1NiJ9.e30.',ip:'::1',emoji:'😀'},{time:'12',jwt:'bad',ip:'127.0.0.1',emoji:'a'}]) {
   const invalid=formats.time==='12';const body={amount:'2',formats,extra:true};
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',formats:{...formats,time:'12:34:56.123'}});
   else assert.ok((await response.text()).includes('formats'));count++;
  }
  for(const mode of ['none','warning','error'])for(const invalid of [false,true]) {
   const body={amount:'2',joined:{left:' a ',right:'2'},...(invalid?{conflict:' x '}:{}),extra:true};
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',joined:{left:'a',right:2}});
   else assert.ok((await response.text()).includes('Intersection results could not be merged'));count++;
  }
  for(const mode of ['none','warning','error'])for(const event of [{kind:'create',label:' a '},{kind:0,count:'2'},{kind:'A',count:'2'},{kind:'missing'}]) {
   const body={amount:'2',event,extra:true};const invalid=event.kind==='A'||event.kind==='missing';
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',event:event.kind===0?{kind:0,count:2}:{kind:'create',label:'a'}});
   else assert.ok((await response.text()).includes(event.kind==='A'?'Invalid enum value':'Invalid discriminator value'));count++;
  }
  for(const mode of ['none','warning','error'])for(const caseText of [' ß ','ﬃ']) {
   const body={amount:'2',caseText,lowerText:'ΟΣ',extra:true};const invalid=caseText==='ﬃ';
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',caseText:'SS',lowerText:'ος'});
   else assert.ok((await response.text()).includes('at most 2'));count++;
  }
  for(const mode of ['none','warning','error'])for(const floatText of ['😀','a']) {
   const body={amount:'2',floatText,nanText:'any length',extra:true};const invalid=floatText==='a';
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',floatText,nanText:'any length'});
   else assert.ok((await response.text()).includes('at least 1.5'));count++;
  }
  for(const mode of ['none','warning','error'])for(const boundedItems of [[' a ',' b '],[' a '],[false]]) {
   const body={amount:'2',boundedItems,extra:true};const invalid=boundedItems.length!==2;
   const response=await fetch(ts+'/transform/'+mode+'/public',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
   assert.equal(response.status,invalid&&mode==='error'?400:200);
   if(response.status===200)assert.deepEqual((await response.json()).body,invalid?body:{amount:2,label:'fallback',boundedItems:['a','b']});
   else assert.ok((await response.text()).includes('array minimum'));count++;
  }
  const mcp=(await(await fetch(ts+'/mcp-origin')).json()).url;
  let registered=0;
  for(const mode of ['none','warning','error'])for(const body of [{amount:'2',items:[{enabled:'true',drop:1}],extra:true},{amount:'bad',items:[{enabled:[]}],extra:true}]) {
   const response=await fetch(mcp+'/mcp',{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name:'Transform'+mode+'public',arguments:{body}}})});
   const result=await response.json();assert.ok(!result.error,JSON.stringify(result));assert.equal(!!result.result.isError,mode==='error'&&body.amount==='bad');
   if(!result.result.isError){const report=JSON.parse(result.result.content[0].text);assert.deepEqual(report.body,body.amount==='bad'?body:{amount:2,label:'fallback',items:[{enabled:true}]});}registered++;
  }
  console.log('PASS '+count+' actual configured TS composition success/atomic failure HTTP cases and '+registered+' registered FastMCP calls');
 } else {
 run('cargo',['build','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--all-features','--example','json_composition_runtime']);
 const ts=await start(process.execPath,['service.mjs']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'framework-rs/target'),'debug/examples/json_composition_runtime'),[]);
 const inputs=[...[{boundedItems:[' a ',' b ']},{boundedItems:[' a ']},{boundedItems:[0,' b ',' c ']}].map(value=>({amount:'2',...value,extra:true})),...[{floatText:'😀',nanText:'arbitrary length'},{floatText:'a'},{floatText:'abc',nanText:0}].map(value=>({amount:'2',...value,extra:true})),...[{caseText:' ß ',lowerText:'ΟΣ'},{caseText:'ﬃ',lowerText:'İ'},{caseText:0},{lowerText:'AΣ\u0345A'}].map(value=>({amount:'2',...value,extra:true})),...[{kind:'create',label:' a ',drop:true},{kind:'update',label:2},{kind:0,count:'2',drop:true},{kind:'A',count:'2'},{kind:'missing'},{}].map(event=>({amount:'2',event,extra:true})),{amount:'2',formats:{time:' 12:34:56.123 ',jwt:'eyJhbGciOiJIUzI1NiJ9.e30.',ip:'::1',emoji:'😀'}},{amount:'2',formats:{time:'12',jwt:'bad',ip:'127.0.0.1',emoji:'a'}},{amount:'12.5',extra:true},{amount:'',label:'explicit',extra:true},{amount:null},{amount:'bad',extra:true},{amount:'2',label:3,extra:true},{amount:'3',items:[{enabled:'true',drop:'secret'}],extra:true},{amount:'3',items:[{enabled:'wrong'},{enabled:[]}]},{amount:'2',choice:'  valid  '},{amount:'2',choice:' x '},{amount:'2',choice:[]},{amount:'2',choice:{nested:false}},{amount:'2',choice:{nested:['true','false'],strip:1}},{amount:'2',kind:'create'},{amount:'2',kind:'update'},{amount:'2',kind:'unknown'},{amount:'2',settings:{enabled:'true'}},{amount:'2',settings:{enabled:'true',z:1,a:2,'10':3,'2':4}},{amount:'bad',settings:{enabled:[],z:1,a:2}},{amount:'2',labels:{' a ':'1',a:'2'}},{amount:'2',labels:{a:'2',' a ':'1'}},{amount:'2',labels:{z:'bad',a:'bad'}},{amount:'2',cleared:null,nothing:null},{amount:'2',cleared:'bad'},{amount:'2',omitted:null},{amount:'2',voided:null},{amount:'2',forbidden:null},{amount:'2',nothing:false},{amount:'2',bag:{label:'ok',z:1,a:2}},{amount:'2',bag:{label:0,z:1}},{amount:'2',scores:{label:'ok',z:'2',a:'3'}},{amount:'2',scores:{label:0,z:'bad',a:'bad'}},{},null,[]];
 inputs.push({amount:'2',joined:{left:' a ',right:'2',extra:true}},{amount:'2',joined:{left:false,right:'bad'}},{amount:'2',conflict:' x '},{amount:'2',conflict:'x'});
 inputs.push({amount:'2',pair:[' a ','2']},{amount:'2',pair:[]},{amount:'2',pair:[false,'bad']},{amount:'2',pair:['a','2',3]},{amount:'2',trail:['a','2','true','false']},{amount:'2',trail:['a','2','wrong']});
 if(process.env.FORKLAUNCH_COMPOSITION_URI==='1') {
  assert.equal(process.versions.node,'24.2.0');assert.equal(process.versions.ada,'3.2.4');
  for(const uri of [' https://example.com/a b ','http://a\u061db.test','http://a\u0c5cb.test','http://a%D8%9Db.test','relative/path','file:///C:/path'])inputs.push({amount:'2',uri});
 }
 let comparisons=0;
 for(const mode of ['none','warning','error']){
  const cases=[...['public','protected'].flatMap(profile=>inputs.map(input=>[profile,JSON.stringify(input),'application/json'])),['default',undefined,undefined],['default','private original bytes','text/plain'],['remove','{}','application/json'],['public','{"amount":"2","settings":{"enabled":"true","z":1,"a":2,"z":3}}','application/json'],['public','{"amount":"2","labels":{" a ":"1","a":"2"," a ":"3","__proto__":"4"}}','application/json']];
  for(const [profile,wire,media] of cases){
   const results=[];
   for(const url of [ts,rust]){
    const before=await(await fetch(url+'/calls')).json();
    const counters=url===rust?await(await fetch(rust+'/counters')).json():null;
    const response=await fetch(url+'/transform/'+mode+'/'+profile,{method:'POST',headers:{...(media?{'content-type':media}:{}),'x-correlation-id':'transform-oracle'},...(wire===undefined?{}:{body:wire})});
    const text=await response.text();const after=await(await fetch(url+'/calls')).json();
    if(counters){const afterCounters=await(await fetch(rust+'/counters')).json();assert.equal(afterCounters.transforms-counters.transforms,1);assert.equal(afterCounters.auth-counters.auth,profile==='protected'&&response.status!==400?1:0);}
    results.push({status:response.status,body:response.status===200?JSON.parse(text):text,handlerCalls:after.handlerCalls-before.handlerCalls});
   }
   assert.deepEqual(results[1],results[0],JSON.stringify({mode,profile,wire,results}));comparisons++;
  }
 }
 console.log(`PASS ${comparisons} actual TypeScript/native reusable JSON composition HTTP comparisons`);
 const tsMcp=(await(await fetch(ts+'/mcp-origin')).json()).url;
 async function rpc(base,method,params) {const response=await fetch(base+'/mcp',{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});assert.equal(response.status,200);return response.json();}
 const nativeTools=(await rpc(rust,'tools/list',{})).result.tools;
 let mcpComparisons=0;
 for(const mode of ['none','warning','error'])for(const input of inputs) {
  const expected=await rpc(tsMcp,'tools/call',{name:'Transform'+mode+'public',arguments:{body:input}});
  const name=nativeTools.find(tool=>tool.name.toLowerCase().includes(('Transform'+mode+'public').toLowerCase()))?.name;
  assert.ok(name,JSON.stringify(nativeTools));
  const before=await(await fetch(rust+'/counters')).json();const handlers=await(await fetch(rust+'/calls')).json();
  const actual=await rpc(rust,'tools/call',{name,arguments:{body:input}});
  const after=await(await fetch(rust+'/counters')).json();assert.equal(after.transforms-before.transforms,1);assert.equal(after.auth,before.auth);
  assert.equal((await(await fetch(rust+'/calls')).json()).handlerCalls-handlers.handlerCalls,expected.result?.isError?0:1);
  assert.equal(!!actual.result?.isError,!!expected.result?.isError,JSON.stringify({mode,input,actual,expected}));
  if(!expected.result?.isError){
   const reference=JSON.parse(expected.result.content[0].text),native=JSON.parse(actual.result.content[0].text);
   assert.deepEqual({hasBody:native.hasBody,body:native.body},{hasBody:reference.hasBody,body:reference.body});
   // Tool transports may order JSON object keys differently; each retained raw body
   // must still represent the original untransformed input, not transformed output.
   assert.deepEqual(JSON.parse(native.raw),input);assert.deepEqual(JSON.parse(reference.raw),input);
  }
  else assert.ok(actual.result.content[0].text.includes('body'));
  mcpComparisons++;
 }
 console.log(`PASS ${mcpComparisons} registered FastMCP/native MCP composition calls (error envelopes scoped to rejection/body diagnostics)`);

 for(const mode of ['none','warning','error']) {
  const before=await(await fetch(rust+'/counters')).json();const handlers=await(await fetch(rust+'/calls')).json();
  const response=await fetch(rust+'/transform/'+mode+'/unsupported',{method:'POST',headers:{'content-type':'application/json'},body:'{"private":"secret"}'});
  assert.equal(response.status,500);assert.ok(!(await response.text()).includes('secret'));
  const after=await(await fetch(rust+'/counters')).json();assert.equal(after.transforms-before.transforms,1);assert.equal(after.auth,before.auth);assert.deepEqual(await(await fetch(rust+'/calls')).json(),handlers);
 }
 console.log('PASS three operational evaluator failures remain safe500 in every validation mode');
 run('cargo',['run','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','typescript_json_composition','--',temporary]);
 writeFileSync(join(temporary,'sdk-check.ts'),`
import assert from 'node:assert/strict';import {createForkLaunchSdk} from './client';
const sdk=createForkLaunchSdk({baseUrl:${JSON.stringify(rust)}});
function find(value:any,name:string):any {for(const [key,item] of Object.entries(value)){if(key.toLowerCase()===name.toLowerCase()&&typeof item==='function')return item.bind(value);if(item&&typeof item==='object'){const result=find(item,name);if(result)return result;}}}
for(const mode of ['none','warning','error']) {const call=find(sdk,'Transform'+mode+'public');assert.ok(call);const result=await call({body:{amount:'2',boundedItems:[' a ',' b '],floatText:'😀',nanText:'any length',caseText:' ß ',lowerText:'ΟΣ',event:{kind:'create',label:' a '},joined:{left:' a ',right:'2'},pair:[' a ','2'],uri:' https://example.com/a b ',formats:{time:' 12:34:56.123 ',jwt:'eyJhbGciOiJIUzI1NiJ9.e30.',ip:'::1',emoji:'😀'},bag:{label:'ok',extra:'kept'},scores:{label:'ok',z:'2'},cleared:null,nothing:null,kind:'create',choice:' good ',settings:{enabled:'true'},labels:{' a ':'1',a:'2'},items:[{enabled:'true',drop:1}],extra:true}});assert.equal(result.status,200);assert.deepEqual(result.response.body,{amount:2,boundedItems:['a','b'],floatText:'😀',nanText:'any length',caseText:'SS',lowerText:'ος',event:{kind:'create',label:'a'},label:'fallback',joined:{left:'a',right:2},pair:['a',2],uri:' https://example.com/a b ',formats:{time:'12:34:56.123',jwt:'eyJhbGciOiJIUzI1NiJ9.e30.',ip:'::1',emoji:'😀'},bag:{label:'ok',extra:'kept'},scores:{label:'ok',z:2},cleared:null,nothing:null,kind:'create',choice:'good',settings:{enabled:true},labels:{a:2},items:[{enabled:true}]});
 const invalid={amount:'2',boundedItems:[false],floatText:'a',caseText:'ﬃ',event:{kind:'A',count:'2'},conflict:' x ',kind:'unknown',choice:{nested:false},settings:{enabled:'true',z:1,a:2},labels:{z:'bad',a:'bad'},formats:{time:'12',jwt:'bad',ip:'127.0.0.1',emoji:'a'},extra:'retained'};const failure=await call({body:invalid});assert.equal(failure.status,mode==='error'?400:200);if(mode!=='error')assert.deepEqual(failure.response.body,invalid);else {assert.ok(failure.response.includes('Union Schema Variant'));assert.ok(failure.response.includes('formats'));assert.ok(failure.response.includes('Invalid enum value'));assert.ok(failure.response.includes('Intersection results could not be merged'));}
}
console.log('PASS six generated TypeScript SDK composition success/failure calls');
`);
 run(join(dependencies,'node_modules/.bin/esbuild'),['sdk-check.ts','--bundle','--platform=node','--format=esm','--packages=external','--outfile=sdk-check.mjs']);run(process.execPath,['sdk-check.mjs']);console.log('PASS six generated TypeScript SDK composition success/failure calls');
 const rustSource=readFileSync(join(temporary,'client.rs'),'utf8');
 const rustCalls=['none','warning','error'].map(mode=>{
  const prefix='Transform'+mode+'public';
  return `let response = client.transform${mode}public_raw(${prefix}Request { body: Some(r#"{"amount":"2","boundedItems":[" a "," b "],"floatText":"😀","nanText":"any length","caseText":" ß ","lowerText":"ΟΣ","event":{"kind":0,"count":"2"},"joined":{"left":" a ","right":"2"},"pair":[" a ","2"],"uri":" https://example.com/a b ","formats":{"time":" 12:34:56.123 ","jwt":"eyJhbGciOiJIUzI1NiJ9.e30.","ip":"::1","emoji":"😀"},"bag":{"label":"ok","extra":"kept"},"scores":{"label":"ok","z":"2"},"cleared":null,"nothing":null,"kind":"create","choice":" good ","settings":{"enabled":"true"},"labels":{" a ":"1","a":"2"},"items":[{"enabled":"true","drop":1}],"extra":true}"#.into()), ..Default::default() }).await.unwrap(); let ${prefix}Responses::Status200(value) = response else { panic!("unexpected response"); }; assert_eq!(value["body"]["amount"].as_f64(),Some(2.0)); assert_eq!(value["body"]["label"],"fallback");assert_eq!(value["body"]["boundedItems"],serde_json::json!(["a","b"]));assert_eq!(value["body"]["floatText"],"😀");assert_eq!(value["body"]["nanText"],"any length");assert_eq!(value["body"]["caseText"],"SS");assert_eq!(value["body"]["lowerText"],"ος");assert_eq!(value["body"]["event"]["kind"].as_f64(),Some(0.0));assert_eq!(value["body"]["event"]["count"].as_f64(),Some(2.0));assert_eq!(value["body"]["joined"]["left"],"a");assert_eq!(value["body"]["joined"]["right"].as_f64(),Some(2.0));assert_eq!(value["body"]["formats"]["time"],"12:34:56.123");assert_eq!(value["body"]["formats"]["ip"],"::1");assert_eq!(value["body"]["pair"][0],"a");assert_eq!(value["body"]["pair"][1].as_f64(),Some(2.0));assert!(value["body"]["cleared"].is_null());assert!(value["body"]["nothing"].is_null());assert!(value["body"].get("omitted").is_none());assert_eq!(value["body"]["uri"]," https://example.com/a b ");assert_eq!(value["body"]["bag"]["extra"],"kept");assert_eq!(value["body"]["scores"]["z"].as_f64(),Some(2.0));assert_eq!(value["body"]["choice"],"good");assert_eq!(value["body"]["kind"],"create");assert_eq!(value["body"]["settings"]["enabled"],true);assert_eq!(value["body"]["labels"]["a"].as_f64(),Some(2.0)); assert!(value["body"].get("extra").is_none());
let invalid=client.transform${mode}public_raw(${prefix}Request {body:Some(r#"{"amount":"2","boundedItems":[false],"floatText":"a","caseText":"ﬃ","event":{"kind":"A","count":"2"},"conflict":" x ","kind":"unknown","choice":{"nested":false},"settings":{"enabled":"true","z":1,"a":2},"labels":{"z":"bad","a":"bad"},"formats":{"time":"12","jwt":"bad","ip":"127.0.0.1","emoji":"a"},"extra":"retained"}"#.into()),..Default::default()}).await.unwrap();
${mode==='error'?`let ${prefix}Responses::Status400(error)=invalid else {panic!("expected rejection")};assert!(error.contains("formats"));assert!(error.contains("Invalid enum value"));assert!(error.contains("Intersection results could not be merged"));`:`let ${prefix}Responses::Status200(value)=invalid else {panic!("expected validation bypass")};assert_eq!(value["body"],serde_json::json!({"amount":"2","boundedItems":[false],"floatText":"a","caseText":"ﬃ","event":{"kind":"A","count":"2"},"conflict":" x ","kind":"unknown","choice":{"nested":false},"settings":{"enabled":"true","z":1,"a":2},"labels":{"z":"bad","a":"bad"},"formats":{"time":"12","jwt":"bad","ip":"127.0.0.1","emoji":"a"},"extra":"retained"}));`}`;
 }).join('\n');
 mkdirSync(join(temporary,'rust-sdk/src'),{recursive:true});
 writeFileSync(join(temporary,'rust-sdk/Cargo.toml'),'[package]\nname="composition-sdk-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'+
  // Generated Rust clients run on the universal-sdk runtime crate.
  'forklaunch-universal-sdk={path='+JSON.stringify(join(root,'framework-rs/crates/universal-sdk'))+'}\nreqwest={version="0.13",default-features=false,features=["json"]}\nserde={version="1",features=["derive"]}\nserde_json="1"\ntokio={version="1",features=["rt-multi-thread","macros"]}\n');
 writeFileSync(join(temporary,'rust-sdk/src/main.rs'),rustSource+'\n#[tokio::main] async fn main(){let client=CompositionClient::new('+JSON.stringify(rust)+');'+rustCalls+'}');
 run('cargo',['run','--quiet','--manifest-path',join(temporary,'rust-sdk/Cargo.toml')]);
 console.log('PASS six generated Rust SDK composition success/failure calls');

 }
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
