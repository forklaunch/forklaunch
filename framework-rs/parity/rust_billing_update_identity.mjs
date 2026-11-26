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
  const initial=kind==='Plan'?{id,externalId,name:'Owned',price:12,cadence:'month',currency:'USD',active:true,billingProvider:'stripe',providerFields:{id:'prod_old'}}:{id,externalId,partyId:'local_customer',partyType:'user',productId:'local_product',active:true,billingProvider:'stripe',startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE',providerFields:{id:'sub_old'}};
  let row=structuredClone(initial),provider;
  const good={id:externalId,object:name,product:'prod_owned',interval:'month',currency:'USD',amount:12,items:{data:[{id:'si_owned',plan:{id:'plan_owned'}}]}};
  const updated={id:kind==='Plan'?'plan_new':externalId,object:name,product:{id:'prod_owned',object:'product'},customer:'cus_distinct_from_local_uuid'};
  const api={async retrieve(target){calls.push('provider:get:'+target);return target==='plan_new'?updated:provider;},async del(target){calls.push('provider:delete:'+target);},async create(fields){calls.push('provider:create');assert.equal(fields.product,'prod_owned');assert.equal(fields.amount,12);return updated;},async update(target,fields){calls.push('provider:update:'+target);assert.deepEqual(structuredClone(fields),{items:[{id:'si_owned',plan:'plan_owned'}]});return updated;}};
  const client={[name==='plan'?'plans':'subscriptions']:api};
  const em={async findOneOrFail(){calls.push('orm:read');return row;},assign(entity,fields){calls.push('orm:assign');Object.assign(entity,fields);},async upsert(entity){calls.push('orm:upsert');return entity;},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
  const service=new Service(client,em,{},actual.validator,configured),method='update'+kind;
  const invalid=[{...good,id:'private_wrong_id'},{...good,id:undefined},{...good,id:1},{...good,id:null},{...good,object:'customer'},{...good,object:undefined},{...good,object:null},null,[]];
  for(const bad of invalid){provider=bad;const input={id},before=structuredClone(input);await assert.rejects(service[method](input),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(row,initial);assert.deepEqual(input,before);assert.deepEqual(calls,['orm:read','provider:get:'+externalId]);calls.length=0;cases++;}
  const sequence=['orm:read','provider:get:'+externalId,...(kind==='Plan'?['provider:delete:'+externalId,'provider:create','provider:get:plan_new']:['provider:update:'+externalId]),'orm:read','orm:assign','orm:upsert','orm:transaction','orm:persist'];
  provider=good;const input={id},before=structuredClone(input);const result=await service[method](input);assert.equal(result.id,id);assert.equal(result.externalId,updated.id);assert.deepEqual(input,before);assert.deepEqual(calls,sequence);calls.length=0;cases++;
  row=structuredClone(initial);provider={...good,id:'custom_provider_shape',object:'custom'};
  const fallback=new Service(client,em,{},actual.validator,{...configured,[kind+'Mapper']:{...configured[kind+'Mapper'],validateProviderRead:undefined}});
  assert.equal((await fallback[method]({id})).id,id);assert.deepEqual(calls,sequence);cases++;
 }
}
console.log(`PASS ${cases} actual configured Plan/Subscription update read preflight cases: malformed GET has one read and no provider/local mutations, valid order and custom fallback unchanged`);
