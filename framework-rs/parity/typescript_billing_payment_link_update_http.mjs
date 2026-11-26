// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');

import {createRequire} from 'node:module';
import {readFileSync,realpathSync} from 'node:fs';
import vm from 'node:vm';
const variant='stripe';
 let actual;
 class FixtureEntityManager {}
 const entities=new Map();
 actual=actualValidator(root,(spec,file,load)=>{
  if(spec==='@forklaunch/validator/zod')return actual.statics;
  if(spec==='@forklaunch/blueprint-core')return {schemaValidator:actual.validator};
  if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};
  if(spec==='@forklaunch/core/cache')return {createCacheKey:prefix=>id=>prefix+':'+id};
  if(spec==='@forklaunch/implementation-billing-base/services')return {...load(path.join(root,'blueprint/implementations/billing/base/services/billingPortal.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/checkoutSession.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/paymentLink.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/plan.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/subscription.service.ts'))};
  if(spec==='@forklaunch/core/mappers')return {...load(path.join(root,'framework/core/src/mappers/mapper.ts')),...load(path.join(root,'framework/core/src/mappers/mapServiceSchemas.ts'))};
  if(spec==='@forklaunch/internal')return load(path.join(root,'framework/internal/src/serviceSchemaResolver.ts'));
  if(spec===`@forklaunch/implementation-billing-${variant}/schemas`)return load(path.join(root,`blueprint/implementations/billing/${variant}/domain/schemas/index.ts`));
  // These imports are present in the resolver barrel but this fixture selects Zod.
  if(spec.startsWith('./typebox/'))return {};
  if(spec.includes('/persistence/entities/')){
   const name=path.basename(spec,'.entity'),exportName=name[0].toUpperCase()+name.slice(1);
   if(!entities.has(name))entities.set(name,{[exportName]:{name}});
   return entities.get(name);
  }
  if(spec==='@mikro-orm/core')return {EntityManager:FixtureEntityManager,wrap:entity=>({toPOJO:()=>({...entity})})};
 });



const require=createRequire(import.meta.url);
const dependencies=process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES;
assert.ok(dependencies,'provide actual canonical dependencies');
const express=require(path.join(dependencies,'node_modules/express'));
const ts=require(require.resolve('typescript',{paths:[path.dirname(realpathSync(process.env.TSC_BIN))]}));
const file=path.join(root,'framework/express/src/expressApplication.ts');
const source=ts.createSourceFile(file,readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
let initializer;
function visit(node){if(ts.isVariableDeclaration(node)&&node.name.getText(source)==='errorHandler')initializer=node.initializer;ts.forEachChild(node,visit);}
visit(source);assert.ok(initializer);
const printed=ts.createPrinter().printNode(ts.EmitHint.Expression,initializer,source);
const compiled=ts.transpileModule(`exports.handler=(function(){return ${printed};}).call({openTelemetryCollector:{error(){}}});`,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function actualModule(relative,imports={}) {
 const exports={};const code=ts.transpileModule(readFileSync(path.join(root,relative),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 vm.runInNewContext(code,{exports,Date,require(spec){if(Object.hasOwn(imports,spec))return imports[spec];return require(path.join(dependencies,'node_modules',spec));}});return exports;
}
const guard=actualModule('framework/core/src/http/guards/isForklaunchRequest.ts');
const contextModule=actualModule('framework/core/src/http/middleware/request/createContext.middleware.ts',{
 'uuid':{v4:randomUUID}, // Owned entropy primitive; actual middleware chooses/propagates it.
 '@forklaunch/common':{getEnvVar:()=>undefined},
 '../../telemetry/constants':{ATTR_CORRELATION_ID:'correlation.id',ATTR_SERVICE_NAME:'service.name'},
 '../../telemetry/openTelemetryCollector':{httpRequestsInFlightCounter:{add(){}}}
});
const box={exports:{},express,console:{error(){}},isForklaunchRequest:guard.isForklaunchRequest,meta:value=>value,ATTR_HTTP_RESPONSE_STATUS_CODE:'http.response.status_code'};
vm.runInNewContext(compiled,box);
const app=express();app.use(express.json());
app.use((req,res,next)=>{if(req.headers['x-owned-context']==='yes'){req.contractDetails={};return contextModule.createContext(actual.validator)(req,res,next);}next();});

const Stripe=require(process.env.STRIPE_SDK_PATH);
const mappers=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/paymentLink.mappers.ts'));
const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/paymentLink.service.ts')).StripePaymentLinkService;
const id=randomUUID();
const good={id,amount:12,currency:'USD',paymentMethods:['card'],status:'PENDING',stripeFields:{id:'plink_retained',object:'payment_link',livemode:false},createdAt:'2026-01-01T00:00:00Z'};
let scenario='missing',providerCalls=0,cacheWrites=0,reads=0,written,providerFields;
const cache={async readRecord(){reads++;if(scenario==='missing')return null;if(scenario==='store')throw Error('private cache');return {value:{...good,...(scenario==='cache-id'?{id:'other'}:{}),...(scenario==='cache-date'?{createdAt:'bad-date'}:{}),...(scenario==='no-stored-id'?{stripeFields:{object:'payment_link',livemode:false}}:{})}};},getTtlMilliseconds:()=>60000,async putRecord({value}){cacheWrites++;if(scenario!=='success')throw Error('unexpected write');written=value;}};
const client={paymentLinks:{async update(target,fields){providerCalls++;providerFields=structuredClone(fields);assert.equal(target,'plink_retained','Stripe is called with the stored Stripe id, never the public id');if(scenario==='success')return {id:target,object:'payment_link',line_items:{data:[{amount_total:34}]}};if(scenario==='timeout')throw new Stripe.errors.StripeConnectionError({message:'private timeout'});throw new Stripe.errors.StripeInvalidRequestError({message:'private provider detail',statusCode:Number(scenario)});}}};
const service=new Service(client,{},cache,{},actual.validator,mappers);
app.put('/paymentLink/:id',async(req,res,next)=>{try{res.json(await service.updatePaymentLink({...req.body,id:req.params.id}));}catch(error){next(error);}});
app.use(box.exports.handler);
const server=await new Promise(resolve=>{const server=app.listen(0,'127.0.0.1',()=>resolve(server));});
try {
 let cases=0;
 const failures=[['missing',{}],['store',{}],['cache-id',{}],['cache-date',{}],['no-stored-id',{}],['patch-status',{status:'INVALID'}],['patch-currency',{currency:'INVALID'}],['patch-method',{paymentMethods:['INVALID']}],['patch-amount',{amount:'bad'}],['patch-null',{amount:null}],['timeout',{}],['404',{}],['429',{}],['500',{}]];
 for(const [failure,patch] of failures)for(const correlation of [undefined,'update-correlation','']) {
  scenario=failure;providerCalls=0;cacheWrites=0;reads=0;
  const headers={'x-owned-context':'yes','content-type':'application/json'};if(correlation!==undefined)headers['x-correlation-id']=correlation;
  const response=await fetch(`http://127.0.0.1:${server.address().port}/paymentLink/${id}`,{method:'PUT',headers,body:JSON.stringify(patch)});
  const expected=['404','429','500'].includes(failure)?Number(failure):500;
  assert.equal(response.status,expected);assert.match(response.headers.get('content-type'),/^text\/plain/);
  const observed=response.headers.get('x-correlation-id');if(correlation)assert.equal(observed,correlation);else assert.match(observed,/^[a-f0-9-]{36}$/);
  assert.equal(await response.text(),`${expected>=500?'Internal server error':'Request could not be processed'}:\n\nCorrelation id: ${observed}`);
  assert.equal(providerCalls,['patch-null','timeout','404','429','500'].includes(failure)?1:0,failure);assert.equal(reads,1);assert.equal(cacheWrites,0);cases++;
 }
 // Execute the actual successful parse middleware and controller callback.
 // The handler registry/bootstrap itself is not instantiated by this fixture.
 const requestShape=actualModule('framework/core/src/http/guards/isRequestShape.ts');
 const parser=actualModule('framework/core/src/http/middleware/request/parse.middleware.ts',{
  '@forklaunch/common':{isRecord(){throw Error('unexpected versioned path');}},
  '@forklaunch/validator':actualModule('framework/validator/src/shared/utils/prettyPrintParseErrors.ts'),
  '../../guards/isRequestShape':requestShape,
  '../../guards/hasSend':actualModule('framework/core/src/http/guards/hasSend.ts'),
  '../internal/validationDiagnostic':actualModule('framework/core/src/http/middleware/internal/validationDiagnostic.ts')
 });
 const controllerPath=path.join(root,'blueprint/billing-stripe/api/controllers/paymentLink.controller.ts');
 const controllerSource=ts.createSourceFile(controllerPath,readFileSync(controllerPath,'utf8'),ts.ScriptTarget.Latest,true);let updateDeclaration;
 function findController(node){if(ts.isVariableDeclaration(node)&&node.name.getText(controllerSource)==='updatePaymentLink')updateDeclaration=node;ts.forEachChild(node,findController);}
 findController(controllerSource);assert.ok(updateDeclaration);assert.equal(updateDeclaration.initializer.arguments[2].properties.find(p=>p.name.getText(controllerSource)==='body').initializer.getText(controllerSource),'UpdatePaymentLinkMapper.schema');
 const callback=ts.createPrinter().printNode(ts.EmitHint.Expression,updateDeclaration.initializer.arguments[3],controllerSource);
 const controllerBox={exports:{},serviceFactory:()=>service,openTelemetryCollector:{debug(){}}};vm.runInNewContext(ts.transpileModule(`exports.handler=${callback};`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,controllerBox);
 for(const amount of [null,'12',false,[],true,'', '  ', '0x10', [12], [[12]], 'Infinity', '-Infinity', '1e999']) {
  scenario='success';providerCalls=0;cacheWrites=0;written=undefined;
  const req={params:{id},body:{id,amount},query:{},headers:{},contractDetails:{},schemaValidator:actual.validator,requestSchema:actual.validator.schemify({body:actual.validator.schemify(mappers.UpdatePaymentLinkMapper.schema),params:actual.validator.schemify({id:actual.statics.string})})};
  let parsed=false;parser.parse(req,{},()=>{parsed=true;});assert.ok(parsed);assert.equal(req.body.amount,Number(amount));assert.equal(req._rawBody.amount,amount);
  let returned;const res={status(value){assert.equal(value,200);return this;},json(value){returned=value;}};await controllerBox.exports.handler(req,res);
  assert.equal(providerCalls,1);assert.equal(providerFields.amount,undefined);assert.equal(written.amount,34);assert.equal(returned.amount,34);assert.equal(cacheWrites,1);cases++;
 }
 // Invalid controls run the same actual parser, including its real diagnostic
 // formatter and response marker. They must never reach the controller/service.
 for(const amount of ['bad', {}, [1,2], {toString:'bad'}]) {
  scenario='success';providerCalls=0;cacheWrites=0;reads=0;
  const req={params:{id},body:{id,amount},query:{},headers:{},context:{correlationId:'parse-control'},contractDetails:{},schemaValidator:actual.validator,requestSchema:actual.validator.schemify({body:actual.validator.schemify(mappers.UpdatePaymentLinkMapper.schema),params:actual.validator.schemify({id:actual.statics.string})})};
  let next=false,status,text,type;
  const res={type(value){type=value;return this;},status(value){status=value;return this;},send(value){text=value;return this;}};
  parser.parse(req,res,()=>{next=true;});
  assert.equal(next,false);assert.equal(status,400);assert.equal(type,'text/plain');
  assert.match(text,/Request validation failed/);assert.match(text,/Path: body > amount/);assert.match(text,/Correlation id: parse-control$/);
  assert.equal(req._rawBody,req.body);assert.equal(req.body.amount,amount);
  assert.equal(providerCalls,0);assert.equal(cacheWrites,0);assert.equal(reads,0);cases++;
 }
 // The actual shared router registers parse before parseRequestAuth.
 const routerSource=readFileSync(path.join(root,'framework/core/src/http/router/routerSharedLogic.ts'),'utf8');
 assert.match(routerSource,/\n    parse,\n    parseRequestAuth</);
 function authSource(relative) {
  const exports={};const code=ts.transpileModule(readFileSync(path.join(root,relative),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  vm.runInNewContext(code,{exports,Date,Buffer,process,require(spec){
   if(spec==='@forklaunch/common')return {...actualModule('framework/common/src/safeStringify.ts'),...actualModule('framework/common/src/guards/isNever.ts')};
   if(spec==='@forklaunch/validator')return actualModule('framework/validator/src/shared/utils/prettyPrintParseErrors.ts');
   if(spec.endsWith('/pinoLogger'))return {meta:value=>value};
   if(spec==='jose')return {jwtVerify(){throw Error('unselected JWT');}};
   if(spec==='crypto')return require('node:crypto');
   if(spec.startsWith('.'))return authSource(path.relative(root,path.resolve(root,path.dirname(relative),spec+'.ts')));
   throw Error('unexpected auth dependency '+spec);
  }});return exports;
 }
 const auth=authSource('framework/core/src/http/middleware/request/auth.middleware.ts');
 for(const amount of [null,'12','bad',{}]) {
  scenario='success';providerCalls=0;cacheWrites=0;reads=0;
  const req={method:'PUT',path:'/paymentLink/'+id,params:{id},body:{id,amount},query:{},headers:{authorization:'HMAC keyId=default ts=2026-10-15T00:00:00.000Z nonce=owned signature=invalid'},context:{correlationId:'auth-control'},contractDetails:{access:'internal',auth:{hmac:{secretKeys:{default:'owned-secret'}}}},schemaValidator:actual.validator,requestSchema:actual.validator.schemify({body:actual.validator.schemify(mappers.UpdatePaymentLinkMapper.schema),params:actual.validator.schemify({id:actual.statics.string})})};
  let parsed=false,authorized=false,status;const res={type(){return this;},status(value){status=value;return this;},send(){return this;}};
  parser.parse(req,res,()=>{parsed=true;});if(parsed)await auth.parseRequestAuth(req,res,()=>{authorized=true;});
  assert.equal(status,amount==='bad'||typeof amount==='object'&&amount!==null?400:403);assert.equal(authorized,false);assert.equal(req._rawBody.amount,amount);assert.equal(providerCalls,0);assert.equal(reads,0);cases++;
 }
 // Live Express seam executes the actual ordered parser/auth/controller chain.
 // Registry/bootstrap and ORM/cache remain owned by this fixture.
 let retained;
 app.put('/parsed/:id',(req,res,next)=>{
  req.contractDetails={access:'internal',auth:{hmac:{secretKeys:{default:'owned-secret'}}}};
  req.schemaValidator=actual.validator;
  req.requestSchema=actual.validator.schemify({body:actual.validator.schemify(mappers.UpdatePaymentLinkMapper.schema),params:actual.validator.schemify({id:actual.statics.string})});
  retained=req.body;
  parser.parse(req,res,next);
 },auth.parseRequestAuth,async(req,res,next)=>{try {assert.equal(req._rawBody,retained);await controllerBox.exports.handler(req,res);}catch(error){next(error);}});
 app.use(box.exports.handler);
 const hmac=authSource('framework/core/src/http/createHmacToken.ts');
 for(const amount of [null,'12',false,[],'Infinity','bad',{}])for(const correlation of [undefined,'parse-wire',''])for(const badSignature of [false,true]) {
  scenario='success';providerCalls=0;cacheWrites=0;reads=0;
  const body={id,amount},route='/parsed/'+id,stamp=new Date(),nonce=randomUUID();
  const signature=badSignature?'invalid':hmac.createHmacToken({method:'PUT',path:route,body,timestamp:stamp,nonce,secretKey:'owned-secret'});
  const headers={'x-owned-context':'yes','content-type':'application/json',authorization:`HMAC keyId=default ts=${stamp.toISOString()} nonce=${nonce} signature=${signature}`};if(correlation!==undefined)headers['x-correlation-id']=correlation;
  const response=await fetch(`http://127.0.0.1:${server.address().port}${route}`,{method:'PUT',headers,body:JSON.stringify(body)});
  const invalid=amount==='bad'||typeof amount==='object'&&amount!==null&&!Array.isArray(amount);
  assert.equal(response.status,invalid?400:badSignature?403:200);
  assert.equal(providerCalls,invalid||badSignature?0:1);assert.equal(cacheWrites,providerCalls);assert.equal(reads,providerCalls*2);
  assert.deepEqual(retained,body);
  if(invalid){const text=await response.text();assert.match(text,/Path: body > amount/);assert.ok(text.endsWith('Correlation id: '+response.headers.get('x-correlation-id')));}
  if(!invalid&&!badSignature){assert.equal((await response.json()).amount,34);assert.equal(providerFields.amount,undefined);assert.equal(written.amount,34);}
  cases++;
 }
 const amountSchema=actual.validator.openapi(mappers.UpdatePaymentLinkMapper.schema).properties.amount;
 assert.deepEqual(Array.from(amountSchema.type),['number'],'actual TS metadata exposes inner Number schema, not all preprocessable wire values');
 console.log(`PASS ${cases} actual configured PaymentLink update service/mapper + actual createContext/final Express handler: cached/missing/invalid patch preflight0provider writes; null amount is admitted before owned provider failure; SDK error-class status404/429/500/connection retained; target is the stored Stripe id while the public ID stays in the API, and a record without one fails before Stripe. Error transport route skips request admission; additional actual parse+controller successes prove null/numeric-string/bool/array/Infinity amount becomes Number(input), and invalid controls return admission400 before service, provider ignores requested amount, and cached/returned amount becomes provider sum34. No full bootstrap claim.`);
}finally{await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}
