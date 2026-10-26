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
let scenario='missing',providerCalls=0;
const id=randomUUID();
for(const [family,serviceName,method] of [['checkoutSession','StripeCheckoutSessionService','getCheckoutSession'],['paymentLink','StripePaymentLinkService','getPaymentLink']]) {
 const mappers=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${family}.mappers.ts`));
 const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${family}.service.ts`))[serviceName];
 const provider=family==='checkoutSession'?{id:'cs_owned',object:'checkout.session',mode:'subscription',livemode:false,customer:null}:{id:'plink_owned',object:'payment_link',livemode:false};
 const dto={id,amount:12,customerId:'local-party',expiresAt:'2026-10-20T00:00:00Z',currency:'USD',paymentMethods:['card'],status:'PENDING',stripeFields:provider};
 const cache={async readRecord(){if(scenario==='store')throw Error('private store failure');if(scenario==='missing')return null;return {value:{...dto,...(scenario==='decode'?{createdAt:'not-a-date'}:{})}};}};
 const retrieve=async()=>{providerCalls++;if(scenario==='malformed')return {};if(scenario==='timeout')throw new Stripe.errors.StripeConnectionError({message:'private timeout'});throw new Stripe.errors.StripeInvalidRequestError({message:'private provider detail',statusCode:Number(scenario)});};
 const client={checkout:{sessions:{retrieve}},paymentLinks:{retrieve}};
 const service=new Service(client,{},cache,{log(){}},actual.validator,mappers);
 app.get(`/${family}/:id`,async(req,res,next)=>{try{res.json(await service[method]({id:req.params.id}));}catch(error){next(error);}});
 if(family==='paymentLink')app.get('/paymentLinkList/:id',async(req,res,next)=>{try{res.json(await service.listPaymentLinks({ids:[req.params.id]}));}catch(error){next(error);}});
}
app.use(box.exports.handler);
const server=await new Promise(resolve=>{const server=app.listen(0,'127.0.0.1',()=>resolve(server));});
try {
 let cases=0;
 for(const family of ['checkoutSession','paymentLink','paymentLinkList'])for(const failure of ['missing','store','decode','malformed','timeout','404','429','500'])for(const correlation of [undefined,'read-correlation','']) {
  scenario=failure;const before=providerCalls;const headers={'x-owned-context':'yes'};if(correlation!==undefined)headers['x-correlation-id']=correlation;
  const response=await fetch(`http://127.0.0.1:${server.address().port}/${family}/${id}`,{headers});
  const expected=['404','429','500'].includes(failure)?Number(failure):500;assert.equal(response.status,expected);assert.match(response.headers.get('content-type'),/^text\/plain/);
  const observed=response.headers.get('x-correlation-id');if(correlation)assert.equal(observed,correlation);else assert.match(observed,/^[a-f0-9-]{36}$/);
  assert.equal(await response.text(),`${expected>=500?'Internal server error':'Request could not be processed'}:\n\nCorrelation id: ${observed}`);
  assert.equal(providerCalls-before,['missing','store','decode'].includes(failure)?0:1);cases++;
 }
 console.log(`PASS ${cases} actual configured GET/list services/mappers + actual createContext/final Express handler: missing/decode/store/SDK provider404/429/500/connection/malformed response sanitized text with actual provider status retention; no full controller/auth/bootstrap claim.`);
}finally{await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}
