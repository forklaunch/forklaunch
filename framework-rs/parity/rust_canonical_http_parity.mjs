import assert from 'node:assert/strict';
import {normalizeOpenApi,sortKeys} from './fixtures/canonical-iam/normalize_openapi.mjs';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,symlinkSync,rmSync} from 'node:fs';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
// Flatten only disjoint, unconstrained primitive alternatives. Overlapping or
// constrained unions carry validation semantics and must survive normalization.
function normalizedSchema(schema){
 return normalizeOpenApi({paths:{'/control':{get:{responses:{200:{content:{'application/json':{schema}}}}}}}})['GET /control'].responses['200']['application/json'];
}
for(const keyword of ['oneOf','anyOf']){
 assert.deepEqual(normalizedSchema({[keyword]:[{type:'string'},{type:'null'}]}),{type:['null','string']});
 for(const branches of [[{type:'string'},{type:'string'}],[{type:'number'},{type:'integer'}],[{type:'string',minLength:1},{type:'null'}]])
  assert.deepEqual(normalizedSchema({[keyword]:branches}),{[keyword]:branches.map(branch=>({...branch,type:[branch.type]}))});
 assert.deepEqual(normalizedSchema({[keyword]:[{type:'string'},{type:'null'}],not:{type:'null'}}),{[keyword]:[{type:['string']},{type:['null']}],not:{type:'null'}});
}
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES (npm install the canonical-iam fixture package)');
const fixture=join(root,'framework-rs/parity/fixtures/canonical-iam');
const require=createRequire(join(dependencies,'package.json'));
const expected=JSON.parse(readFileSync(join(fixture,'package.json'))).dependencies;
for(const [name,version] of Object.entries(expected)) {
 assert.equal(JSON.parse(readFileSync(join(dependencies,'node_modules',name,'package.json'))).version,version,`${name} fixture version`);
}
const {SignJWT,jwtVerify}=await import(pathToFileURL(require.resolve('jose')));
const temporary=mkdtempSync(join(tmpdir(),'fl-canonical-http-'));
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
  '@fixture/openapi':['source/core/src/http/openApiV3Generator/openApiV3Generator.ts'],
  '@forklaunch/common':['source/common/index.ts'],'@forklaunch/validator':['source/validator/index.ts'],
  '@forklaunch/validator/*':['source/validator/src/*/index.ts'],'@forklaunch/core/*':['source/core/src/*/index.ts'],
  '@forklaunch/express':['source/express/index.ts']},target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true}}));
 cpSync(join(fixture,'service.ts'),join(temporary,'service.ts'));
 cpSync(join(fixture,'openapi.ts'),join(temporary,'openapi.ts'));
 run(join(dependencies,'node_modules/.bin/esbuild'),['openapi.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=openapi.mjs']);
 run(process.execPath,['openapi.mjs']);
 run(join(dependencies,'node_modules/.bin/esbuild'),['service.ts','--bundle','--platform=node','--format=esm','--packages=external','--tsconfig=tsconfig.json','--outfile=service.mjs']);
 run('cargo',['build','--quiet','--manifest-path',join(root,'blueprint-rs/Cargo.toml'),'-p','forklaunch-iam-base','--example','canonical_iam']);
 const ts=await start(process.execPath,['service.mjs']);
 const rust=await start(join(process.env.CARGO_TARGET_DIR??join(root,'blueprint-rs/target'),'debug/examples/canonical_iam'),[join(temporary,'issued.json')]);
 const key=new TextEncoder().encode('test-only-signing-key');
 const issued=JSON.parse(readFileSync(join(temporary,'issued.json')));
 const verified=await jwtVerify(issued.token,key,{algorithms:['HS256']});
 assert.deepEqual(verified.payload,issued.claims);
 assert.equal(verified.protectedHeader.alg,'HS256');
 const token=await new SignJWT({sub:'tester'}).setProtectedHeader({alg:'HS256'}).setExpirationTime('1h').sign(key);
 const expired=await new SignJWT({sub:'tester',exp:1}).setProtectedHeader({alg:'HS256'}).sign(key);
 const now=Math.floor(Date.now()/1000);
 const recentlyExpired=await new SignJWT({sub:'tester',exp:now-30}).setProtectedHeader({alg:'HS256'}).sign(key);
 const notYetValid=await new SignJWT({sub:'tester',exp:now+7200,nbf:now+3600}).setProtectedHeader({alg:'HS256'}).sign(key);
 const forged=await new SignJWT({sub:'tester'}).setProtectedHeader({alg:'HS256'}).setExpirationTime('1h').sign(new TextEncoder().encode('wrong-key'));
 const users=[{id:'user_1',email:'ada@example.com',display_name:null,disabled:false,roles:[]},{id:'user_2',email:'grace@example.com',display_name:null,disabled:false,roles:[]}];
 const numericDates=[];
 for(const [extra,status] of [[{nbf:null},403],[{nbf:'0'},403],[{iat:null},403],[{iat:'0'},403],[{nbf:0.5,iat:now+3600},200]]){
  const signed=await new SignJWT({sub:'tester',exp:now+7200,...extra}).setProtectedHeader({alg:'HS256'}).sign(key);
  numericDates.push(['/users',signed,status,...(status===200?[{items:users,next_cursor:null}]:[])]);
 }
 const cases=[
  ['/users/user_1',token,200,users[0]],['/users/user_1',issued.token,200,users[0]],['/users/user_2',token,200,users[1]],
  ['/users',token,200,{items:users,next_cursor:null}],
  ['/users?limit=2&cursor=user_1',token,200,{items:users,next_cursor:null}],
  ['/users/missing',token,404,''],['/users',undefined,401],['/users','invalid',403],
  ['/users',expired,403],['/users',forged,403],
  ['/users',recentlyExpired,403],['/users',notYetValid,403],...numericDates
 ];
 for(const [path,bearer,status,body] of cases){
  const wire=[];
  for(const [name,url] of [['TypeScript',ts],['Rust',rust]]){
   const response=await fetch(url+path,{headers:bearer?{authorization:`Bearer ${bearer}`}:{},signal:AbortSignal.timeout(5000)});
   const text=await response.text();assert.equal(response.status,status,`${name} ${path}: ${text}`);
   wire.push({text,type:response.headers.get('content-type')});
   if(body!==undefined)assert.deepEqual(text?JSON.parse(text):'',body,`${name} ${path}`);
  }
  if(status===401||status===403){assert.deepEqual(wire[1],wire[0],`authentication wire parity: ${path}`);assert.equal(wire[0].type,'text/plain; charset=utf-8');}
 }
 const headerCases=[['Bearer',401],[`bearer ${token}`,401],[`Basic ${token}`,401],
  [`Bearer  ${token}`,403],[`Bearer ${token} ignored`,200],[`Bearer ${token}  suffix`,200]];
 for(const [authorization,status] of headerCases){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(`${url}/users`,{headers:{authorization},signal:AbortSignal.timeout(5000)});
   assert.equal(response.status,status);
   wire.push({text:await response.text(),type:response.headers.get('content-type')});
  }
  if(status!==200)assert.deepEqual(wire[1],wire[0],'exact auth header/error wire profile');
  else assert.deepEqual(JSON.parse(wire[1].text),JSON.parse(wire[0].text));
 }
 // Exercise the real Express final error handler, not a fixture formatter.
 for(const correlation of ['fixture-error-123','with spaces','punctuation:._-']){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(`${url}/errors/throw`,{headers:{'x-correlation-id':correlation},signal:AbortSignal.timeout(5000)});
   assert.equal(response.status,500);
   const text=await response.text();
   assert.equal(text,`Internal server error:\n\nCorrelation id: ${correlation}`);
   assert.ok(!text.includes('secret fixture detail'));
   wire.push({text,type:response.headers.get('content-type')});
  }
  assert.deepEqual(wire[1],wire[0],'generic handler exception wire parity');
  assert.equal(wire[0].type,'text/plain; charset=utf-8');
 }
 console.log('PASS six live actual TypeScript/Rust handler exception outcomes: safe text, media type and correlation ID');
 for(const displayName of ['', ' ', '\t', 'Ada']){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(`${url}/errors/validate?display_name=${encodeURIComponent(displayName)}`,{
    headers:{'x-correlation-id':'validation-fixture'},signal:AbortSignal.timeout(5000)});
   const body=await response.text();
   assert.equal(response.status,displayName.trim()?200:400);
   if(!displayName.trim()){
    assert.equal(body,'Request validation failed with the following errors:\n1. Path: query > display_name\n   Message: A display name is required\n\nCorrelation id: validation-fixture');
    assert.equal(response.headers.get('content-type'),'text/plain; charset=utf-8');
    assert.throws(()=>JSON.parse(body),'upstream request error is plain text');
   }else assert.equal(body,'accepted');
   wire.push({body,type:response.headers.get('content-type')});
  }
  // Only compare the validation rejection media type: native success text
  // defaults to text/plain without the Express UTF-8 suffix.
  if(!displayName.trim())assert.deepEqual(wire[1],wire[0],'request validation wire parity');
 }
 console.log('PASS eight live actual TypeScript/Rust domain-validation outcomes: rejection body/media/correlation and valid requests');
 let parserOutcomes=0;
 for(const path of ['/parser/axum','/parser/local']){
  for(const [body,contentType,deny,status] of [
   ['null','application/json',false,400],
   ['{"task":"PRIVATE_BODY"','application/json',false,400],
   [JSON.stringify({task:'x'.repeat(80)}),'application/json',false,413],
   [JSON.stringify({task:'x'.repeat(160)}),'application/json',false,413],
   ['{"task":"ok"}','application/json; charset=unsupported',false,415],
   ['{"task":"ok"}','application/json',true,403],
   ['{"task":"ok"}','application/json',false,200],
  ]){
   const wire=[];
   for(const url of [ts,rust]){
    const before=await (await fetch(url+'/parser/calls')).json();
    const response=await fetch(url+path,{method:'POST',headers:{'content-type':contentType,'x-correlation-id':'parser-fixture',...(deny?{'x-deny':'yes'}:{})},body,signal:AbortSignal.timeout(5000)});
    const text=await response.text();assert.equal(response.status,status,`${path}: ${text}`);
    const after=await (await fetch(url+'/parser/calls')).json();
    assert.equal(after-before,status===200?1:0,'rejected parser input must not reach handler');
    if(status!==200){
     assert.equal(text,'Request could not be processed:\n\nCorrelation id: parser-fixture');
     assert.doesNotMatch(text,/PRIVATE|SyntaxError|verification/i);
     assert.equal(response.headers.get('content-type'),'text/plain; charset=utf-8');
    }else assert.deepEqual(JSON.parse(text),{accepted:'yes'});
    wire.push(text);parserOutcomes++;
   }
   assert.equal(wire[1],wire[0],'final Express/native parser wire parity');
  }
 }
 console.log(`PASS ${parserOutcomes} live parser outcomes: strict JSON, malformed input, parser/transport limits, charset, verifier, safe messages and handler isolation`);
 for(const path of ['/users/%FF','/users/%ZZ','/users/%C0%AF','/users/%E0%A4%A']){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(url+path,{headers:{authorization:`Bearer ${token}`,'x-correlation-id':'uri-fixture'},signal:AbortSignal.timeout(5000)});
   assert.equal(response.status,400);
   const body=await response.text();
   assert.equal(body,'Request could not be processed:\n\nCorrelation id: No correlation ID');
   assert.equal(response.headers.get('content-type'),'text/plain; charset=utf-8');
   wire.push(body);
  }
  assert.equal(wire[1],wire[0],'malformed URI fails before TypeScript context creation');
 }
 console.log('PASS eight live malformed percent/UTF8 URI outcomes with exact pre-context error envelope');
 const queryCases=[['limit=oops',400],['limit=1.5',400],['limit=Infinity',400],['limit=1&limit=2',400],['cursor=a&cursor=b',400],['limit=bad&cursor=a&cursor=b',400],['limit=',200],['limit=%20',200],['limit=0x10',200],['limit=1e2',200],['limit=-1',200],['cursor=',200],['unknown=ignored',200],['unknown=a&unknown=b',200],['limit=%C2%851',400],['limit=1%C2%85',400],['limit=%EF%BB%BF1',200],['limit=1%EF%BB%BF',200]];
 for(const [query,status] of queryCases){
  const wire=[];
  for(const [name,url] of [['TypeScript',ts],['Rust',rust]]){
   const before=await (await fetch(`${url}/query/calls`)).json();
   const response=await fetch(`${url}/users?${query}`,{headers:{authorization:`Bearer ${token}`,'x-correlation-id':'query-fixture'},signal:AbortSignal.timeout(5000)});
   const text=await response.text();
   assert.equal(response.status,status,`${name} query ${query}: ${text}`);
   wire.push({text,type:response.headers.get('content-type')});
   const after=await (await fetch(`${url}/query/calls`)).json();
   assert.equal(after-before,status===200?1:0,`${name} query rejected before handler`);
  }
  if(status===400)assert.deepEqual(wire[1],wire[0],`query error wire parity: ${query}`);
  else assert.deepEqual(JSON.parse(wire[1].text),JSON.parse(wire[0].text));
 }
 console.log('PASS 36 live schema-bound query outcomes including repeated/empty/unknown values and zero rejected handler calls');
 for(const path of ['/response/axum/','/response/local/']){
  for(const mode of ['wrong','headers','both','empty','valid']){
   const wire=[];
   for(const url of [ts,rust]){
    const response=await fetch(url+path+mode,{headers:{'x-correlation-id':'response-fixture'},signal:AbortSignal.timeout(5000)});
    wire.push({status:response.status,text:await response.text(),type:response.headers.get('content-type'),result:response.headers.get('x-result')});
   }
   assert.equal(wire[0].status,mode==='valid'?200:500);
   // Successful native text omits the charset; error diagnostics and empty500 are exact.
   if(mode==='valid')assert.deepEqual({...wire[1],type:wire[0].type},wire[0]);
   else assert.deepEqual(wire[1],wire[0],`response validation ${path}${mode}`);
  }
 }
 console.log('PASS20live response-validator outcomes: trusted body/header diagnostics, ordering, local/Axum and explicit empty500 preservation');
 // Two independent failures followed by valid output ensure no shared validation mutation.
 for(const adapter of ['axum','local'])for(const mode of ['invalid','invalid','client','client','server','server','valid','declared']){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(`${url}/status/${adapter}/${mode}`,{headers:{'x-correlation-id':'status-fixture'}});
   wire.push({status:response.status,text:await response.text(),type:response.headers.get('content-type')});
  }
  assert.deepEqual(wire[1],wire[0]);
  assert.equal(wire[0].status,mode==='valid'?200:mode==='client'?418:mode==='server'?503:500);
  assert.equal(wire[0].text,['valid','declared'].includes(mode)?'accepted':`${mode==='client'?'Request could not be processed':'Internal server error'}:\n\nCorrelation id: status-fixture`);
 }
 console.log('PASS 32 live undocumented-status/profile outcomes including repeated redacted418/503 and declared500 preservation');
 let defaultOutcomes=0;
 const defaultKinds=['text','jsonstring','object','number','binary','empty','emptytext'];
 for(const adapter of ['axum','local'])for(const status of [400,401,403,404,500])for(const kind of defaultKinds){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(`${url}/defaults/${adapter}/${status}/${kind}`,{headers:{'x-correlation-id':'default-fixture'}});
   wire.push({status:response.status,type:response.headers.get('content-type'),body:await response.text()});defaultOutcomes++;
  }
  assert.deepEqual(wire[1],wire[0],`default error ${adapter}/${status}/${kind}`);
  assert.equal(wire[0].status,['object','number','binary'].includes(kind)?500:status);
  assert.ok(!wire[0].body.includes('PRIVATE'));
 }
 for(const [path,status] of [['/users',404],['/users',500],['/users/user_1',500]])for(const kind of defaultKinds){
  const wire=[];
  for(const url of [ts,rust]){
   const response=await fetch(url+path,{headers:{authorization:`Bearer ${token}`,'x-correlation-id':'default-fixture','x-default-status':String(status),'x-default-kind':kind}});
   wire.push({status:response.status,type:response.headers.get('content-type'),body:await response.text()});defaultOutcomes++;
  }
  assert.deepEqual(wire[1],wire[0],`documented default ${path}/${status}/${kind}`);
 }
 console.log(`PASS ${defaultOutcomes} live selected default error-string outcomes, including actual documented routes`);
 const normalized=[];
 for(const [url,path] of [[ts,'/api/openapi'],[rust,'/openapi.json']]){
  const response=await fetch(url+path,{signal:AbortSignal.timeout(5000)});assert.equal(response.status,200);
  const document=await response.json();const spec=document.latest??document;
  if(process.env.FORKLAUNCH_TEST_OPENAPI_OUTPUT){
   mkdirSync(process.env.FORKLAUNCH_TEST_OPENAPI_OUTPUT,{recursive:true});
   writeFileSync(join(process.env.FORKLAUNCH_TEST_OPENAPI_OUTPUT,url===ts?'typescript.json':'rust.json'),JSON.stringify(spec,null,2)+'\n');
  }
  const name=url===ts?'typescript':'rust';
  const snapshot=JSON.stringify(sortKeys(spec),null,2)+'\n';
  const snapshotPath=join(fixture,`${name}.openapi.json`);
  if(process.env.UPDATE_CANONICAL_OPENAPI==='1')writeFileSync(snapshotPath,snapshot);
  assert.equal(snapshot,readFileSync(snapshotPath,'utf8'),`${name} OpenAPI snapshot drift`);
  normalized.push(normalizeOpenApi(spec));
  assert.ok(spec.paths['/users']);assert.ok(spec.paths['/users/{id}']);
  assert.equal(spec.paths['/users/{id}'].get.parameters.find(parameter=>parameter.in==='path').required,true);
  for(const item of Object.values(spec.paths))for(const operation of Object.values(item))
   for(const requirement of operation.security??[])for(const name of Object.keys(requirement))
    assert.ok(spec.components.securitySchemes[name],`unresolved security scheme ${name}`);
 }
 assert.deepEqual(Object.keys(normalized[0]),Object.keys(normalized[1]));
 const differences=[];
 for(const operation of Object.keys(normalized[0])){
  for(const field of ['parameters','security'])assert.deepEqual(normalized[0][operation][field],normalized[1][operation][field],`${operation} ${field}`);
  assert.deepEqual(normalized[0][operation].responses['200'],normalized[1][operation].responses['200'],`${operation} success schema`);
  for(const status of ['401','403'])assert.deepEqual(normalized[0][operation].responses[status],normalized[1][operation].responses[status],`${operation} documented auth error schema`);
  const left=normalized[0][operation].responses,right=normalized[1][operation].responses;
  for(const status of [...new Set([...Object.keys(left),...Object.keys(right)])].sort()){
   if(JSON.stringify(left[status])!==JSON.stringify(right[status]))differences.push({operation,status,typescript:left[status]??null,rust:right[status]??null});
  }
 }
 const differenceSnapshot=JSON.stringify(differences,null,2)+'\n';
 const differencePath=join(fixture,'openapi-differences.json');
 if(process.env.UPDATE_CANONICAL_OPENAPI==='1')writeFileSync(differencePath,differenceSnapshot);
 assert.equal(differenceSnapshot,readFileSync(differencePath,'utf8'),'remaining OpenAPI response differences changed');
 console.log(`PASS normalized OpenAPI parameters/security/success schemas; ${differences.length} explicit error-response document differences retained`);
 console.log(`PASS canonical actual TypeScript/Rust IAM services: ${(cases.length+headerCases.length)*2} live HTTP outcomes and both OpenAPI endpoints; rejected-JWT statuses, exact text and media types match`);
}finally{
 for(const child of children){
  if(child.exitCode!==null||child.signalCode!==null)continue;
  await new Promise(resolve=>{const timer=setTimeout(()=>child.kill('SIGKILL'),5000);child.once('exit',()=>{clearTimeout(timer);resolve();});child.kill('SIGTERM');});
 }
 rmSync(temporary,{recursive:true,force:true});
}
