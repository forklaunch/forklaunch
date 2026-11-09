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
 for(const partyType of ['user','organization']) for(const id of [randomUUID(),'legacy-local-id']) {
  const calls=[],missing=new Error('Missing subscription');
  const good={id,partyId:'local-owner',partyType,productId:'local-product',active:true,externalId:'sub_owned',startDate:new Date('2026-01-01'),status:'active',billingProvider:'stripe',providerFields:{id:'sub_owned',customer:'cus_provider',nested:{date:'2026-01-01'}}};
  let row=structuredClone(good);
  const em={async findOneOrFail(_e,where){calls.push('orm:read');if(where.id==='missing')throw missing;assert.equal(where.id,id);return row;},assign(entity,fields){calls.push('orm:assign');Object.assign(entity,fields);},async upsert(entity){calls.push('orm:upsert');return entity;},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
  const client={subscriptions:{async retrieve(target){calls.push('stripe:retrieve:'+target);return {id:target,object:'subscription',items:{data:[{id:'si_owned',plan:{id:'plan_owned'}}]}};},async update(target,options){calls.push('stripe:update:'+target);assert.equal(target,'sub_owned');assert.equal(JSON.stringify(options),JSON.stringify({metadata:{note:'kept'},items:[{id:'si_owned',plan:'plan_owned'}]}));return {id:target,object:'subscription',customer:'cus_provider'};}}};
  const service=new Service(client,em,{},actual.validator,configured);
  for(const patch of [{partyId:null},{partyType:null},{partyId:'other-owner'},{partyType:partyType==='user'?'organization':'user'},{partyId:''},{startDate:'invalid-date'},{endDate:'invalid-date'},{active:{}},{productId:{}},{billingProvider:'invalid'}]) {
   calls.length=0;const input={id,...patch},before=structuredClone(input);await assert.rejects(service.updateSubscription(input));assert.deepEqual(input,before);assert.deepEqual(row,good);assert.deepEqual(calls,['orm:read']);cases++;
  }
  for(const change of [{id:randomUUID()},{id:' '},{partyId:' '}]) {
   row={...good,...change};calls.length=0;const before=structuredClone(row);await assert.rejects(service.updateSubscription({id}));assert.deepEqual(row,before);assert.deepEqual(calls,['orm:read']);cases++;
  }
  for(const ownership of [{},{partyId:'local-owner',partyType}]) {
   row=structuredClone(good);calls.length=0;const input={id,...ownership,stripeFields:{metadata:{note:'kept'}}},before=structuredClone(input);
   const result=await service.updateSubscription(input);assert.equal(result.id,id);assert.equal(result.partyId,'local-owner');assert.equal(result.partyType,partyType);assert.equal(result.externalId,'sub_owned');assert.equal(result.stripeFields.customer,'cus_provider');assert.deepEqual(input,before);assert.deepEqual(calls,['orm:read','stripe:retrieve:sub_owned','stripe:update:sub_owned','orm:read','orm:assign','orm:upsert','orm:transaction','orm:persist']);cases++;
  }
  for(const partyId of ['\u0085','\ufeff','\u2028']) {
   row={...structuredClone(good),partyId};calls.length=0;
   if(partyId==='\u0085') {const value=await service.updateSubscription({id,stripeFields:{metadata:{note:'kept'}}});assert.equal(value.partyId,partyId);assert.ok(calls.includes('stripe:update:sub_owned'));}
   else {await assert.rejects(service.updateSubscription({id}));assert.deepEqual(calls,['orm:read']);}
   cases++;
  }
  // Explicit JS undefined passes preflight, but actual response mapper validation
  // fails after the provider effect and local assignment/persistence.
  // JSON HTTP/MCP cannot represent this distinct own-property state.
  row=structuredClone(good);calls.length=0;
  await assert.rejects(service.updateSubscription({id,partyId:undefined,partyType:undefined,stripeFields:{metadata:{note:'kept'}}}));
  assert.ok(calls.includes('stripe:update:sub_owned'));assert.ok(calls.includes('orm:assign'));assert.ok(calls.includes('orm:persist'));cases++;
  calls.length=0;row=structuredClone(good);await assert.rejects(service.updateSubscription({id:'missing',partyId:'other'}),error=>error===missing);assert.deepEqual(calls,['orm:read']);cases++;
  calls.length=0;row=structuredClone(good);const stop=new Error('legacy provider seam');
  const fallback=new Service({subscriptions:{async retrieve(target){calls.push('stripe:retrieve:'+target);throw stop;}}},em,{},actual.validator,{...configured,UpdateSubscriptionMapper:{...configured.UpdateSubscriptionMapper,validateUpdate:undefined}});
  await assert.rejects(fallback.updateSubscription({id,partyId:'other'}),error=>error===stop);assert.deepEqual(calls,['orm:read','stripe:retrieve:sub_owned']);cases++;
 }
}
console.log(`PASS ${cases} actual configured Stripe Subscription preflight cases: user/organization ownership preservation, invalid schema and mismatched local identity deny before provider I/O/writes, omitted/same owner accepted and custom fallback retained`);
