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

 for(const mode of ['createPlan','updatePlan','updateSubscription']){
  const kind=mode.endsWith('Plan')?'Plan':'Subscription',name=kind[0].toLowerCase()+kind.slice(1);
  const configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${name}.service.ts`))['Stripe'+kind+'Service'];
  const id=randomUUID(),externalId=name+'_owned',createdId=kind==='Plan'?'plan_new':externalId,calls=[];
  const initial=kind==='Plan'?{id,externalId,name:'Owned',price:12,cadence:'month',currency:'USD',active:true,billingProvider:'stripe',providerFields:{id:'prod_old'}}:{id,externalId,partyId:'local_customer',partyType:'user',productId:'local_product',active:true,billingProvider:'stripe',startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE',providerFields:{id:'sub_old'}};
  let row=structuredClone(initial),provider;
  const existing={id:externalId,object:name,product:'prod_owned',interval:'month',currency:'USD',amount:12,items:{data:[{id:'si_owned',plan:{id:'plan_owned'}}]}};
  const good={id:createdId,object:name,product:{id:'prod_owned',object:'product'},customer:'cus_distinct_from_local_uuid'};
  const api={async retrieve(target){calls.push('provider:get:'+target);return target==='plan_new'?provider:existing;},async del(target){calls.push('provider:delete:'+target);},async create(){calls.push('provider:create');return {id:createdId,object:'plan'};},async update(target){calls.push('provider:update:'+target);return provider;}};
  const client={[name==='plan'?'plans':'subscriptions']:api};
  const em={create(_entity,fields){calls.push('orm:create');return {id,...fields};},async findOneOrFail(){calls.push('orm:read');return row;},assign(entity,fields){calls.push('orm:assign');Object.assign(entity,fields);},async upsert(entity){calls.push('orm:upsert');return entity;},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
  const service=new Service(client,em,{},actual.validator,configured);
  const input=mode==='createPlan'?{name:'Owned',price:12,cadence:'month',currency:'USD',active:true}:{id};
  const effects=mode==='createPlan'?['provider:create','provider:get:plan_new']:['orm:read','provider:get:'+externalId,...(kind==='Plan'?['provider:delete:'+externalId,'provider:create','provider:get:plan_new']:['provider:update:'+externalId])];
  const invalid=[{...good,id:'private_wrong_id'},{...good,id:undefined},{...good,id:1},{...good,id:null},{...good,object:'customer'},{...good,object:undefined},{...good,object:null},null,[]];
  for(const bad of invalid){provider=bad;const before=structuredClone(input);await assert.rejects(service[mode](input),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(row,initial);assert.deepEqual(input,before);assert.deepEqual(calls,effects,'provider effects occurred once; no persistence, retry or compensation');calls.length=0;cases++;}
  const writes=mode==='createPlan'?['orm:create','orm:transaction','orm:persist']:['orm:read','orm:assign','orm:upsert','orm:transaction','orm:persist'];
  provider=good;const before=structuredClone(input),result=await service[mode](input);assert.equal(result.id,id);assert.equal(result.externalId,createdId);assert.deepEqual(input,before);assert.deepEqual(calls,[...effects,...writes]);calls.length=0;cases++;
  row=structuredClone(initial);provider={...good,id:'legacy_response_reference',object:'custom'};
  const fallback=new Service(client,em,{},actual.validator,{...configured,[kind+'Mapper']:{...configured[kind+'Mapper'],validateProviderRead:undefined}});
  assert.equal((await fallback[mode](input)).externalId,'legacy_response_reference');assert.deepEqual(calls,[...effects,...writes]);cases++;
 }
}
console.log(`PASS ${cases} actual configured post-effect identity cases: provider effects occur, mismatched response prevents local persistence, no retry/compensation, valid UUID/input/sequence and custom fallback retained`);
