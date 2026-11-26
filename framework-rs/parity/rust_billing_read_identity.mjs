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

 for(const kind of ['Plan','Subscription']){
  const name=kind[0].toLowerCase()+kind.slice(1),configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${name}.service.ts`))['Stripe'+kind+'Service'];
  const id=randomUUID(),externalId=name+'_owned',calls=[];
  const row=kind==='Plan'?{id,externalId,name:'Owned',price:12,cadence:'month',currency:'USD',active:true,billingProvider:'stripe',providerFields:{id:'prod_old'}}:{id,externalId,partyId:'local_customer',partyType:'user',productId:'local_product',active:true,billingProvider:'stripe',startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE',providerFields:{id:'sub_old'}};
  let provider;const queued=[];
  const good={id:externalId,object:name,product:{id:'prod_owned',object:'product'},customer:'cus_distinct_from_local_uuid',metadata:{note:'preserved'}};
  const client={[name==='plan'?'plans':'subscriptions']:{async retrieve(target){calls.push('provider:get');assert.equal(target,externalId);return queued.length?queued.shift():provider;}}};
  const em={async findOneOrFail(){calls.push('orm:read');return row;},async findAll(){calls.push('orm:list');return [row];}};
  const service=new Service(client,em,{},actual.validator,configured);
  const methods=kind==='Plan'?['getPlan']:['getSubscription','getUserSubscription','getOrganizationSubscription'];
  const invalid=[{...good,id:'private_wrong_id'},{...good,id:undefined},{...good,id:1},{...good,id:null},{...good,object:'customer'},{...good,object:undefined},{...good,object:null},null,[]];
  for(const method of methods){
   for(const bad of invalid){provider=bad;const before=structuredClone(row);await assert.rejects(service[method]({id}),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(row,before);assert.deepEqual(calls,['orm:read','provider:get']);calls.length=0;cases++;}
   provider=good;const result=await service[method]({id});assert.equal(result.id,id);assert.equal(result.externalId,externalId);assert.deepEqual(result.stripeFields,kind==='Plan'?good.product:good);assert.deepEqual(calls,['orm:read','provider:get']);calls.length=0;cases++;
  }
  if(kind==='Plan'){
   provider={...good,amount:999999,nickname:'Remote name',product:{...good.product,name:'Fresh product',metadata:{note:'new'}}};
   const before=structuredClone(row),result=await service.getPlan({id});
   assert.equal(result.id,id);assert.equal(result.name,row.name);assert.equal(result.price,row.price);
   assert.equal(result.externalId,externalId);assert.deepEqual(result.stripeFields,provider.product);
   assert.notDeepEqual(result.stripeFields,provider);assert.deepEqual(row,before);
   assert.deepEqual(calls,['orm:read','provider:get']);calls.length=0;cases++;
  }
  if(kind==='Subscription'){
   provider={...good,status:'canceled',metadata:{fresh:true}};
   const before=structuredClone(row),result=await service.getSubscription({id});
   assert.equal(result.id,id);assert.equal(result.active,row.active);assert.equal(result.status,row.status);
   assert.equal(result.partyId,row.partyId);assert.equal(result.productId,row.productId);
   assert.deepEqual(result.stripeFields,provider);assert.deepEqual(row,before);
   assert.deepEqual(calls,['orm:read','provider:get']);calls.length=0;cases++;
  }
  const list=kind==='Plan'?'listPlans':'listSubscriptions';
  for(const bad of invalid){provider=bad;assert.deepEqual(structuredClone(await service[list]()),[]);assert.deepEqual(calls,['orm:list','provider:get']);calls.length=0;cases++;}
  provider=good;assert.equal((await service[list]())[0].id,id);calls.length=0;cases++;
  const originalFindAll=em.findAll;em.findAll=async()=>{calls.push('orm:list');return [{...row,id:randomUUID()},row];};queued.push({...good,id:'wrong'},good);const retained=await service[list]();assert.equal(retained.length,1);assert.equal(retained[0].id,id);assert.deepEqual(calls,['orm:list','provider:get','provider:get']);calls.length=0;em.findAll=originalFindAll;cases++;
  const fallback=new Service(client,em,{},actual.validator,{...configured,[kind+'Mapper']:{...configured[kind+'Mapper'],validateProviderRead:undefined}});
  provider={...good,id:'legacy_custom_reference'};assert.equal((await fallback[methods[0]]({id})).id,id);assert.deepEqual(calls,['orm:read','provider:get']);cases++;
 }
}
console.log(`PASS ${cases} actual configured Plan/Subscription read identity cases: exact object/id, sanitized mismatch, local UUID/provider payload preserved, no writes, list skip behavior and custom fallback`);
