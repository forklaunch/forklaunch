// Actual request-auth/context/final-error middleware; owned Express wiring only.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,realpathSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import vm from 'node:vm';
import {randomUUID} from 'node:crypto';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url),deps=process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES;
const ts=require(require.resolve('typescript',{paths:[path.dirname(realpathSync(process.env.TSC_BIN))]}));
const express=require(path.join(deps,'node_modules/express'));
function source(relative) {
 const exports={};const code=ts.transpileModule(readFileSync(path.join(root,relative),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
 vm.runInNewContext(code,{exports,Date,Buffer,process,require(spec){
  if(spec==='@forklaunch/common')return {...source('framework/common/src/safeStringify.ts'),...source('framework/common/src/guards/isNever.ts'),getEnvVar:()=>undefined};
  if(spec==='@forklaunch/validator')return source('framework/validator/src/shared/utils/prettyPrintParseErrors.ts');
  if(spec.endsWith('/pinoLogger'))return {meta:value=>value};
  if(spec.endsWith('/telemetry/constants'))return {ATTR_CORRELATION_ID:'correlation.id',ATTR_SERVICE_NAME:'service.name'};
  if(spec.endsWith('/openTelemetryCollector'))return {httpRequestsInFlightCounter:{add(){}}};
  if(spec==='jose')return {jwtVerify(){throw Error('unselected JWT');}};
  if(spec==='crypto')return require('node:crypto');
  if(spec==='uuid')return {v4:randomUUID};
  if(spec.startsWith('.'))return source(path.relative(root,path.resolve(root,path.dirname(relative),spec+'.ts')));
  return require(path.join(deps,'node_modules',spec));
 }});return exports;
}
for(const family of ['paymentLink','subscription','plan','checkoutSession']) {
 const text=readFileSync(path.join(root,`blueprint/billing-stripe/api/controllers/${family}.controller.ts`),'utf8');
 assert.match(text,/access: 'internal'/);assert.match(text,/hmac:\s*{\s*secretKeys:\s*{\s*default: HMAC_SECRET_KEY/);
}
const portal=readFileSync(path.join(root,'blueprint/billing-stripe/api/controllers/billingPortal.controller.ts'),'utf8');assert.match(portal,/access: 'protected'/);assert.match(portal,/jwt:/);
const auth=source('framework/core/src/http/middleware/request/auth.middleware.ts');
const context=source('framework/core/src/http/middleware/request/createContext.middleware.ts');
const hmac=source('framework/core/src/http/createHmacToken.ts');
const file=path.join(root,'framework/express/src/expressApplication.ts');const tree=ts.createSourceFile(file,readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);let handler;
function visit(node){if(ts.isVariableDeclaration(node)&&node.name.getText(tree)==='errorHandler')handler=node.initializer;ts.forEachChild(node,visit);}visit(tree);assert(handler);
const expression=ts.createPrinter().printNode(ts.EmitHint.Expression,handler,tree);
const box={exports:{},express,console:{error(){}},meta:value=>value,ATTR_HTTP_RESPONSE_STATUS_CODE:'http.response.status_code',...source('framework/core/src/http/guards/isForklaunchRequest.ts')};
vm.runInNewContext(ts.transpileModule(`exports.handler=(function(){return ${expression};}).call({openTelemetryCollector:{error(){}}});`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,box);
const app=express();app.use(express.json({verify(req,_res,buffer){req._rawBody=buffer;}}));
let calls=0;app.use((req,res,next)=>{req.contractDetails={access:'internal',auth:{hmac:{secretKeys:{default:'owned-hmac-secret'}}}};context.createContext({})(req,res,next);});
const guarded=async(req,res,next)=>{try{await auth.parseRequestAuth(req,res,next);}catch(error){next(error);}};
app.put('/owned',guarded,(_req,res)=>{calls++;res.json({ok:true});});
const mounted=express.Router();mounted.put(['/:id','/'],guarded,(_req,res)=>{calls++;res.json({ok:true});});mounted.get(['/:id','/'],guarded,(_req,res)=>{calls++;res.json({ok:true});});mounted.options('/explicit/:id',guarded,(_req,res)=>{calls++;res.status(202).send('explicit');});mounted.get('/:id/success',guarded,(_req,res)=>{calls++;res.json({ok:true});});app.use('/payment-link',mounted);const outer=express.Router();outer.use('/payment-link',mounted);app.use('/outer',outer);app.use(box.exports.handler);
const server=await new Promise(resolve=>{const server=app.listen(0,'127.0.0.1',()=>resolve(server));});
function header(stamp=new Date(),nonce='repeat-nonce') {return `HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${hmac.createHmacToken({method:'PUT',path:'/owned',body:'{"amount":"12"}',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'})}`;}
const valid=header();
function representedTimestamp(value){const stamp=new Date(value),nonce='representation';return `HMAC keyId=default ts=${value} nonce=${nonce} signature=${hmac.createHmacToken({method:'PUT',path:'/owned',body:'{"amount":"12"}',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'})}`;}

const cases=[['missing',undefined,401,'Authorization token required.'],['empty','',401,'Invalid Authorization token format.'],['no-parts','HMAC',401,'Invalid Authorization token format.'],['missing-parts','HMAC keyId=default',403,'Invalid Authorization token.'],['wrong-prefix',valid.replace('HMAC','OTHER'),403,'Invalid Authorization token.'],['wrong-label',valid.replace('keyId=','key='),401,'Invalid Authorization token format.'],['bad-signature',valid.replace(/signature=.*/, 'signature=invalid'),403,'Invalid Authorization signature.'],['unknown-key',valid.replace('keyId=default','keyId=other'),500],['invalid-date',valid.replace(/ts=\S+/, 'ts=not-a-date'),500],['valid',valid,200],['repeated',valid,200],['old',header(new Date(Date.now()-3600000)),200],['future',header(new Date(Date.now()+3600000)),200],['extra-part',valid+' ignored=extra',200],['reordered',valid.replace(/keyId=(\S+) ts=(\S+)/,'ts=$2 keyId=$1'),401,'Invalid Authorization token format.'],['empty-nonce',valid.replace(/nonce=\S+/,'nonce='),401,'Invalid Authorization token format.'],['empty-date',valid.replace(/ts=\S+/,'ts='),401,'Invalid Authorization token format.'],['date-only',representedTimestamp('2026-10-15'),200],['offset',representedTimestamp('2026-10-15T07:23:45.678+02:00'),200],['short-fraction',representedTimestamp('2026-10-15T07:23:45.6Z'),200],['long-fraction',representedTimestamp('2026-10-15T07:23:45.678999Z'),200],['nonce-equals',header(new Date(),'a=b'),200],['legacy-date',representedTimestamp('10/15/2026'),200],['offsetless-date',representedTimestamp('2026-10-15T07:23:45'),200],['full-timeclip-min',representedTimestamp('-271821-04-20T00:00:00.000Z'),200],['full-timeclip-max',representedTimestamp('+275760-09-13T00:00:00.000Z'),200]];
for(const stamp of ['15-Oct-2026','2026/10/15','2026-1-2','2026-03-08T02:30:00','2026-11-01T01:30:00','2026-10-15T24:00','2026-02-31','1','2026-01-01T00:00:00.00000000123456789Z'])cases.push([`date-${stamp}`,representedTimestamp(stamp),200]);
try {
 let count=0;
 for(const [name,authorization,status,body] of cases)for(const correlation of [undefined,'auth-correlation','']) {
  calls=0;const headers={'content-type':'application/json'};if(authorization!==undefined)headers.authorization=authorization;if(correlation!==undefined)headers['x-correlation-id']=correlation;
  const response=await fetch(`http://127.0.0.1:${server.address().port}/owned`,{method:'PUT',headers,body:'{"amount":"12"}'});
  assert.equal(response.status,status,name);assert.equal(calls,status===200?1:0,name);
  const text=await response.text();if(body)assert.equal(text,body,name);else if(status===500)assert.equal(text,`Internal server error:\n\nCorrelation id: ${response.headers.get('x-correlation-id')}`,name);else assert.equal(text,'{"ok":true}',name);
  count++;
 }
 for(const signedPath of ['/owned','/payment-link/owned']) {
  const stamp=new Date(),nonce='mounted';const signature=hmac.createHmacToken({method:'PUT',path:signedPath,body:'{"amount":"12"}',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}/payment-link/owned`,{method:'PUT',headers:{'content-type':'application/json',authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`},body:'{"amount":"12"}'});
  // The full wire path is canonical; the deprecated router-relative form is still accepted by default.
  assert.equal(response.status,200,`mounted signed path ${signedPath}`);assert.equal(calls,1);await response.text();count++;
 }
 // Everything below checks strict path canonicalization: only the full wire path verifies.
 process.env.HMAC_ACCEPT_MOUNT_RELATIVE_PATHS='false';
 {
  for(const signedPath of ['/owned','/payment-link/owned']) {
   const stamp=new Date(),nonce='strict';const signature=hmac.createHmacToken({method:'PUT',path:signedPath,body:'{"amount":"12"}',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
   calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}/payment-link/owned`,{method:'PUT',headers:{'content-type':'application/json',authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`},body:'{"amount":"12"}'});
   assert.equal(response.status,signedPath==='/owned'?403:200,`strict signed path ${signedPath}`);assert.equal(calls,signedPath==='/owned'?0:1);await response.text();count++;
  }
 }
 for(const [wirePath,signedPath,status] of [['/payment-link/abc%2Fdef?x=1','/payment-link/abc%2Fdef',200],['/payment-link/abc%2Fdef?x=1','/payment-link/abc/def',403],['/payment-link/owned?x=1','/payment-link/owned?x=1',403],['/payment-link/owned/','/payment-link/owned/',200],['/payment-link/owned/','/payment-link/owned',403],['/payment-link','/payment-link',200],['/payment-link/','/payment-link/',200],['/payment-link/owned//','/payment-link/owned//',404],['/PAYMENT-LINK/owned','/PAYMENT-LINK/owned',200],['/payment-link/owned','/owned',403],['/payment-link','/',403]]) {
  const stamp=new Date(),nonce='path';const signature=hmac.createHmacToken({method:'PUT',path:signedPath,body:'{"amount":"12"}',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}${wirePath}`,{method:'PUT',headers:{'content-type':'application/json',authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`},body:'{"amount":"12"}'});
  assert.equal(response.status,status,`${wirePath} signed ${signedPath}`);assert.equal(calls,status===200?1:0);await response.text();count++;
 }
 for(const raw of [undefined,'{"z":1,"a":2}',' { "amount" : "12" } ','{"value":"雪"}']) {
  const stamp=new Date(),nonce='raw';const signature=hmac.createHmacToken({method:'PUT',path:'/owned',body:raw,timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  const headers={authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`};if(raw!==undefined)headers['content-type']='application/json';
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}/owned`,{method:'PUT',headers,body:raw});assert.equal(response.status,200);assert.equal(calls,1);await response.text();count++;
 }
 for(const wirePath of ['/payment-link/owned','/payment-link/owned/','/payment-link','/payment-link/','/outer/payment-link/owned/']) {
  const signedPath=wirePath;const stamp=new Date(),nonce='head';
  const signature=hmac.createHmacToken({method:'HEAD',path:signedPath,timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}${wirePath}`,{method:'HEAD',headers:{authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`}});assert.equal(response.status,200);assert.equal(calls,1);assert.equal(await response.text(),'');count++;
 }
 for(const wirePath of ['/payment-link/owned','/payment-link/owned/','/payment-link','/payment-link/','/outer/payment-link/owned/'])for(const authorization of [undefined,'invalid','HMAC keyId=missing ts=invalid nonce=x signature=bad']) {
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}${wirePath}`,{method:'OPTIONS',headers:authorization===undefined?{}:{authorization}});assert.equal(response.status,200);assert.equal(response.headers.get('allow'),'GET, HEAD, PUT');assert.equal(response.headers.get('content-type'),'text/plain');assert.equal(response.headers.get('x-content-type-options'),'nosniff');assert.equal(response.headers.get('content-length'),String(Buffer.byteLength('GET, HEAD, PUT')));assert.equal(calls,0);assert.equal(await response.text(),'GET, HEAD, PUT');count++;
 }
 for(const authorization of [undefined,'invalid','valid']) {
  const stamp=new Date(),nonce='explicit';const signature=hmac.createHmacToken({method:'OPTIONS',path:'/payment-link/explicit/owned',timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  const value=authorization==='valid'?`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`:authorization;
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}/payment-link/explicit/owned`,{method:'OPTIONS',headers:value===undefined?{}:{authorization:value}});assert.equal(response.status,authorization==='valid'?202:401);assert.equal(calls,authorization==='valid'?1:0);assert.equal(response.headers.get('allow'),null);await response.text();count++;
 }

 for(const [wirePath,signedPath,status] of [
 ['/PAYMENT-LINK/MiXeD/SUCCESS','/PAYMENT-LINK/MiXeD/SUCCESS',200],
 ['/PaYmEnT-LiNk/MiXeD/sUcCeSs?Keep=VaLuE','/PaYmEnT-LiNk/MiXeD/sUcCeSs',200],
 ['/PaYmEnT-LiNk/MiXeD/sUcCeSs','/payment-link/mixed/success',403],
 ['/PaYmEnT-LiNk/MiXeD/sUcCeSs','/MiXeD/sUcCeSs',403],
 ['/PAYMENT-LINK/MiXeD%2FaBc/SUCCESS/','/PAYMENT-LINK/MiXeD%2FaBc/SUCCESS/',200],
 ['/PAYMENT-LINK/MiXeD%2FaBc/SUCCESS','/PAYMENT-LINK/MiXeD/aBc/SUCCESS',403],
 ['/%70ayment-link/MiXeD/success','/MiXeD/success',404],
 ['/payment-link/MiXeD/%73uccess','/MiXeD/%73uccess',404],
 ['/payment-linK/MiXeD/success','/MiXeD/success',404],
 ['/payment-link/MiXeD/ſuccess','/MiXeD/ſuccess',404],
 ]) {
  const stamp=new Date(),nonce='case';const signature=hmac.createHmacToken({method:'GET',path:signedPath,timestamp:stamp,nonce,secretKey:'owned-hmac-secret'});
  calls=0;const response=await fetch(`http://127.0.0.1:${server.address().port}${wirePath}`,{headers:{authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`}});assert.equal(response.status,status,wirePath);assert.equal(calls,status===200?1:0);await response.text();count++;
 }
 console.log(`PASS ${count} actual HMAC HTTP context/auth/final-error cases: missing/empty/format/signature/key/date, accepted repeated/old/future/extra-part; raw Buffer signing. Server registry and protected handler are owned; no live provider.`);
}finally{await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}

// Ordered route wiring controls use the same installed Express implementation.
{
const app=express();
const mark=name=>(req,res)=>res.set('X-Selected',name).set('X-Original',req.originalUrl).set('X-Method',req.method).send(name);
app.get('/head/:id',mark('earlier-get'));
app.head('/head/:id',mark('later-head'));
app.head('/explicit/:id',mark('earlier-head'));
app.get('/explicit/:id',mark('later-get'));
app.get('/overlap/:id',mark('earlier-param'));
app.get('/overlap/fixed',mark('later-static'));
app.post('/overlap/fixed',mark('post-static'));
app.put('/overlap/:id',mark('put-param'));
app.post('/method/:id',mark('earlier-post'));
app.get('/method/fixed',mark('later-get-method'));
app.get('/case',mark('lower-first'));
app.get('/CASE',mark('upper-second'));
app.get('/subscription/user/:id',mark('user-first'));
app.get('/subscription/:id/cancel',mark('cancel-second'));
app.options('/explicit-options/:id',mark('options-explicit'));
app.get('/explicit-options/:id',mark('options-get'));
const server=app.listen(0,'127.0.0.1'); await new Promise(r=>server.once('listening',r));
let count=0;
try {
 const base=`http://127.0.0.1:${server.address().port}`;
 for(const [method,path,chosen] of [
 ['HEAD','/HEAD/MiXeD','earlier-get'],['HEAD','/EXPLICIT/MiXeD','earlier-head'],
 ['GET','/OVERLAP/FIXED','earlier-param'],['GET','/overlap/fixed','earlier-param'],
 ['POST','/OVERLAP/FIXED','post-static'],['GET','/METHOD/FIXED','later-get-method'],
 ['GET','/CASE','lower-first'],['GET','/case','lower-first'],
 ['GET','/SUBSCRIPTION/USER/CANCEL','user-first'],
 ['OPTIONS','/EXPLICIT-OPTIONS/MiXeD','options-explicit'],
 ['GET','/OVERLAP/%2F?ignored=yes','earlier-param']]) {
  const result=await fetch(base+path,{method});assert.equal(result.status,200);assert.equal(result.headers.get('x-selected'),chosen);assert.equal(result.headers.get('x-original'),path);assert.equal(result.headers.get('x-method'),method);if(method==='HEAD')assert.equal(await result.text(),'');count++;
 }
 const response=await fetch(base+'/OVERLAP/FIXED',{method:'OPTIONS',headers:{authorization:'invalid'}});
 assert.equal(response.status,200);assert.equal(response.headers.get('allow'),'GET, HEAD, POST, PUT');assert.equal(await response.text(),'GET, HEAD, POST, PUT');count++;
 console.log(`Actual installed Express ordered routing ${count} PASS; owned route wiring, no controller parity claim`);
} finally {await new Promise(r=>server.close(r));}

}

// Direct installed Express path-to-regexp matching, distinct from URL encoding by fetch.
{
 const {match}=require(path.join(deps,'node_modules/path-to-regexp'));
 for(const [literal,candidate,expected] of [
  ['café','CAFÉ',true],['Σ','ς',true],['s','ſ',false],['k','K',false],
  ['i','İ',false],['ß','SS',false],['𐐀','𐐨',false],['%c3%a9','%C3%A9',true],
  ['café','caf%C3%A9',false],['payment-link','%70ayment-link',false],
 ]) assert.equal(Boolean(match(`/${literal}/:id`)(`/${candidate}/MiXeD`)),expected,`${literal} / ${candidate}`);
 console.log('PASS10 installed Express matcher Unicode/non-u and encoded-literal observations; not browser URL-decoding claims');
}
