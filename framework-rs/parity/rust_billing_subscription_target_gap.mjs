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


 const configured=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/subscription.mappers.ts'));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/subscription.service.ts')).StripeSubscriptionService;
 const calls=[];let writes=0;
 const client={subscriptions:{async create(fields){calls.push(structuredClone(fields));if(!fields.customer.startsWith('cus_')||!fields.items[0].plan.startsWith('plan_'))throw new Error('strict fixture rejects unbound local identifiers');return{id:'sub_owned',object:'subscription'};}}};
 const em={create(){writes++;throw Error('unexpected local allocation');},persist(){writes++;},transactional(){writes++;}};
 const service=new Service(client,em,{},actual.validator,configured);
 const input={partyId:randomUUID(),partyType:'user',productId:randomUUID(),active:true,startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE',externalId:'ignored',billingProvider:'stripe'};
 const before=structuredClone(input);
 await assert.rejects(service.createSubscription(input),/strict fixture rejects unbound local identifiers/);
 assert.deepEqual(calls,[{customer:input.partyId,items:[{plan:input.productId}]}]);
 assert.equal(writes,0);assert.deepEqual(input,before);cases++;
}
console.log(`PASS ${cases} actual configured Subscription unresolved UUID-target boundary: UUIDs reach provider unchanged, strict provider rejects, no local record; this is a known compatibility gap, not ownership resolution`);
