// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');

let cases=0;
{ const variant='stripe';
 let actual;
 class FixtureEntityManager {}
 const entities=new Map();
 actual=actualValidator(root,(spec,file,load)=>{
  if(spec==='@forklaunch/validator/zod')return actual.statics;
  if(spec==='@forklaunch/blueprint-core')return {schemaValidator:actual.validator};
  if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};
  if(spec==='@forklaunch/core/cache')return {createCacheKey:prefix=>id=>prefix+':'+id};
  if(spec==='@forklaunch/implementation-billing-base/services')return {...load(path.join(root,'blueprint/implementations/billing/base/services/billingPortal.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/checkoutSession.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/paymentLink.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/plan.service.ts'))};
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

 const configured=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/plan.mappers.ts'));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/plan.service.ts')).StripePlanService;
 const id=randomUUID(),calls=[];
 const product={id:'prod_owned',object:'product',name:'Owned'};
 const em={create(_entity,fields){calls.push('orm:create');return {id,...fields};},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
 let sent;
 const client={plans:{async create(options){calls.push('stripe:create');sent=structuredClone(options);return {id:'plan_owned',object:'plan'};},async retrieve(target,options){calls.push('stripe:retrieve');assert.equal(target,'plan_owned');assert.deepEqual(structuredClone(options),{expand:['product']});return {id:target,object:'plan',product};}}};
 const service=new Service(client,em,{},actual.validator,configured);
 const valid={name:'Owned',price:12,cadence:'month',currency:'USD',active:true,stripeFields:{metadata:{note:'kept',nested:'unchanged'}}};
 for(const patch of [{price:'invalid'},{cadence:'INVALID'},{currency:'INVALID'},{active:{}},{features:[1]},{name:{}},{description:null},{id:{}},{name:undefined},{active:undefined}]) {
  const input={...valid,...patch},before=structuredClone(input);
  await assert.rejects(service.createPlan(input),/Invalid plan creation/);
  assert.deepEqual(input,before);assert.deepEqual(calls,[],JSON.stringify(patch));cases++;
 }
 for(const supplied of [{},{externalId:'caller_id',billingProvider:'other'},{externalId:{untrusted:true},billingProvider:null}]) {
  const input={...valid,...supplied},before=structuredClone(input);
  const result=await service.createPlan(input);
  assert.deepEqual(calls,['stripe:create','stripe:retrieve','orm:create','orm:transaction','orm:persist']);
  assert.deepEqual(sent,{metadata:{note:'kept',nested:'unchanged'},amount:12,interval:'month',product:'Owned',currency:'USD'});
  assert.equal(result.externalId,'plan_owned');assert.equal(result.billingProvider,'stripe');assert.equal(result.id,id);assert.deepEqual(result.stripeFields,product);assert.deepEqual(input,before);calls.length=0;cases++;
 }
 const stop=new Error('owned fallback seam');
 const fallback=new Service({plans:{async create(){calls.push('stripe:create');throw stop;}}},em,{},actual.validator,{...configured,CreatePlanMapper:{...configured.CreatePlanMapper,validateCreate:undefined}});
 await assert.rejects(fallback.createPlan({...valid,price:'invalid'}),error=>error===stop);
 assert.deepEqual(calls,['stripe:create']);cases++;
}
console.log(`PASS ${cases} actual configured Stripe Plan create preflight cases: invalid input performs zero ORM/provider operations, valid creation sequence and provider-owned overrides unchanged, input unchanged and custom fallback retained`);
