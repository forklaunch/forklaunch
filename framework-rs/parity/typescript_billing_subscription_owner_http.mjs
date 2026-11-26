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

 const configured=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/subscription.mappers.ts'));

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
let providerCalls=0;
const id=randomUUID();
app.put('/subscription/:id',async(req,res,next)=>{
 try {await configured.UpdateSubscriptionMapper.validateUpdate({id:req.params.id,...req.body},{id,partyId:'owner',partyType:'user'});providerCalls++;res.json({ok:true});}catch(error){next(error);}
});
app.use(box.exports.handler);
const server=await new Promise(resolve=>{const server=app.listen(0,'127.0.0.1',()=>resolve(server));});
try {
 const origin=`http://127.0.0.1:${server.address().port}`;
 let cases=0;
 for(const contextual of [false,true]) for(const correlation of [undefined,'supplied-correlation','']) for(const body of [{partyId:'other'},{partyType:'organization'}]){
  const headers={'content-type':'application/json'};if(contextual)headers['x-owned-context']='yes';if(correlation!==undefined)headers['x-correlation-id']=correlation;
  const response=await fetch(`${origin}/subscription/${id}`,{method:'PUT',headers,body:JSON.stringify(body)});
  assert.equal(response.status,500);assert.match(response.headers.get('content-type'),/^text\/plain/);
  const observed=contextual?response.headers.get('x-correlation-id'):'No correlation ID';
  if(contextual){if(correlation)assert.equal(observed,correlation);else assert.match(observed,/^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/);}
  const text=await response.text();assert.equal(text,`Internal server error:\n\nCorrelation id: ${observed}`);assert.ok(!text.includes('ownership'));cases++;
 }
 assert.equal(providerCalls,0);
 console.log(`PASS ${cases} actual configured mapper + actual createContext/guard/final-error handler over real Express HTTP: 500 sanitized text, supplied/missing/empty correlation; owned route, entropy primitive and telemetry, not full application bootstrap.`);
} finally {await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}
