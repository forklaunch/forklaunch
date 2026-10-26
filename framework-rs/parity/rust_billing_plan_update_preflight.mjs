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
 let row={id,name:'Owned',price:12,cadence:'month',currency:'USD',externalId:'plan_owned',billingProvider:'stripe',active:true,providerFields:product};
 const em={async findOneOrFail(_e,where){calls.push('orm:read');assert.equal(where.id,id);return row;},assign(entity,fields){calls.push('orm:assign');Object.assign(entity,fields);},async upsert(entity){calls.push('orm:upsert');return entity;},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
 const client={plans:{async retrieve(target,options){calls.push('stripe:retrieve:'+target);return target==='plan_owned'?{id:target,object:'plan',product:'prod_owned',interval:'month',currency:'USD',amount:12}:{id:target,object:'plan',product};},async del(target){calls.push('stripe:delete:'+target);},async create(options){calls.push('stripe:create');assert.equal(options.product,'prod_owned');assert.equal(options.amount,34);assert.equal(options.interval,'month');assert.equal(options.currency,'USD');assert.equal(options.metadata.note,'kept');return {id:'plan_new',object:'plan'};}}};
 const service=new Service(client,em,{},actual.validator,configured);
 for(const patch of [{price:'invalid'},{cadence:'INVALID'},{currency:'INVALID'},{active:{}},{features:[1]},{name:{}},{description:null},{externalId:{}},{billingProvider:'other'},{id:{}}]) {
  const input={id,...patch},before=structuredClone(input);await assert.rejects(service.updatePlan(input));assert.deepEqual(input,before);assert.deepEqual(calls,[],JSON.stringify(patch));cases++;
 }
 const input={id,price:34,stripeFields:{metadata:{note:'kept'}}},before=structuredClone(input);
 const result=await service.updatePlan(input);
 assert.deepEqual(calls,['orm:read','stripe:retrieve:plan_owned','stripe:delete:plan_owned','stripe:create','stripe:retrieve:plan_new','orm:read','orm:assign','orm:upsert','orm:transaction','orm:persist']);
 assert.equal(result.externalId,'plan_new');assert.equal(result.id,id);assert.equal(result.price,34);assert.equal(result.name,'Owned');assert.deepEqual(input,before);cases++;
 calls.length=0;row={...row,externalId:'plan_owned'};
 const named={id,name:'Renamed',price:34,stripeFields:{metadata:{note:'kept'}}},namedBefore=structuredClone(named);
 const renamed=await service.updatePlan(named);assert.equal(renamed.name,'Renamed');assert.equal(renamed.externalId,'plan_new');assert.deepEqual(named,namedBefore);assert.deepEqual(calls,['orm:read','stripe:retrieve:plan_owned','stripe:delete:plan_owned','stripe:create','stripe:retrieve:plan_new','orm:read','orm:assign','orm:upsert','orm:transaction','orm:persist']);cases++;
 calls.length=0;
 const stop=new Error('owned fallback seam');
 const fallback=new Service({plans:{async retrieve(target){calls.push('stripe:retrieve:'+target);throw stop;}}},em,{},actual.validator,{...configured,UpdatePlanMapper:{...configured.UpdatePlanMapper,validateUpdate:undefined}});
 await assert.rejects(fallback.updatePlan({id,price:'invalid'}),error=>error===stop);
 assert.deepEqual(calls,['orm:read','stripe:retrieve:plan_new']);cases++;
}
console.log(`PASS ${cases} actual configured Stripe Plan update preflight cases: invalid patches perform zero ORM/provider operations, valid replacement sequence unchanged, input unchanged and custom fallback retained`);
