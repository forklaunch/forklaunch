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

 const profiles=[
  {name:'subscription',kind:'Subscription',method:'createSubscription',valid:{stripeFields:{metadata:{note:'kept'}},partyId:'cus_owned',partyType:'user',productId:'plan_owned',active:true,startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE'},patches:[{partyId:{}},{partyType:'invalid'},{productId:1},{active:{}},{startDate:'bad'},{endDate:'bad'},{status:1},{description:null},{id:{}}],overrides:{externalId:{ignored:true},billingProvider:null},provider:{id:'sub_owned',object:'subscription'}},
  {name:'checkoutSession',kind:'CheckoutSession',method:'createCheckoutSession',valid:{stripeFields:{metadata:{note:'kept'}},customerId:'cus_owned',currency:'USD',paymentMethods:['card'],status:'PENDING'},patches:[{customerId:{}},{currency:'invalid'},{paymentMethods:['invalid']},{status:'invalid'},{successRedirectUri:{}},{cancelRedirectUri:1}],overrides:{id:{ignored:true},uri:[],expiresAt:'invalid'},provider:{id:'cs_owned',url:'https://example.test/checkout',object:'checkout.session'}},
  {name:'billingPortal',kind:'BillingPortal',method:'createBillingPortalSession',valid:{stripeFields:{metadata:{note:'kept'}},customerId:'cus_owned'},patches:[{customerId:{}},{customerId:null},{customerId:undefined}],overrides:{id:{ignored:true},uri:[],expiresAt:'invalid'},provider:{id:'bps_owned',url:'https://example.test/portal',object:'billing_portal.session'}},
  {name:'paymentLink',kind:'PaymentLink',method:'createPaymentLink',valid:{amount:12,currency:'USD',paymentMethods:['card'],status:'PENDING',stripeFields:{line_items:[{price:'price_owned',quantity:1}],metadata:{note:'kept'}}},patches:[{amount:'invalid'},{amount:NaN},{currency:'invalid'},{paymentMethods:['invalid']},{status:'invalid'},{stripeFields:{}},{stripeFields:{line_items:[null]}}],overrides:{id:{ignored:true}},provider:{id:'plink_owned',object:'payment_link',line_items:{data:[{amount_total:7},{amount_total:5}]}}}
 ];
 profiles.push({name:'plan',kind:'Plan',method:'createPlan',valid:{name:'Owned',price:12,cadence:'month',currency:'USD',active:true},provider:{id:'plan_new',object:'plan',product:{id:'prod_owned',object:'product'}}});
 for(const profile of profiles){
  const configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${profile.name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${profile.name}.service.ts`))['Stripe'+profile.kind+'Service'];
  const id=randomUUID(),calls=[];let provider;
  const em=new FixtureEntityManager();Object.assign(em,{create(_entity,fields){calls.push('orm:create');return {id,...fields};},async transactional(fn){calls.push('orm:transaction');return fn(em);},persist(){calls.push('orm:persist');return {async flush(){calls.push('orm:flush');}};}});
  const cache={async putRecord(){calls.push('cache:put');},getTtlMilliseconds(){return 60000;}};
  const create=async()=>{calls.push('provider:create');return provider;};
  const client={plans:{create,async retrieve(target){calls.push('provider:get');return {...profile.provider,id:target};}},subscriptions:{create},checkout:{sessions:{create}},billingPortal:{sessions:{create}},paymentLinks:{create}};
  const durable=['plan','subscription'].includes(profile.name);
  function build(mappers=configured,backup=false){return durable?new Service(client,em,{},actual.validator,mappers):new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});}
  const invalid=[null,[],{}, {...profile.provider,id:undefined},{...profile.provider,id:null},{...profile.provider,id:7},{...profile.provider,id:''},{...profile.provider,id:'   '},{...profile.provider,id:'\ufeff'},{...profile.provider,id:'bad\u0000id'},{...profile.provider,id:'bad\u0085id'},{...profile.provider,id:'x'.repeat(1025)},{...profile.provider,id:'é'.repeat(513)},{...profile.provider,object:'customer'},{...profile.provider,object:undefined},{...profile.provider,object:null}];
  for(const backup of durable?[false]:[false,true]){
   const service=build(configured,backup);
   for(const bad of invalid){provider=bad;const before=structuredClone(profile.valid);await assert.rejects(service[profile.method](profile.valid),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(profile.valid,before);assert.deepEqual(calls,['provider:create'],'effect occurred but no follow-up GET, cache/ORM allocation, retry or compensation');calls.length=0;cases++;}
   for(const validId of [profile.provider.id,'legacy-custom-id','x'.repeat(1024),'é'.repeat(512)]){provider={...profile.provider,id:validId};const before=structuredClone(profile.valid);const result=await service[profile.method](profile.valid);assert.equal(result.id,id);assert.deepEqual(profile.valid,before);assert.deepEqual(calls,['provider:create',...(profile.name==='plan'?['provider:get']:[]),'orm:create',...(durable?['orm:transaction','orm:persist']:backup?['orm:persist','orm:flush']:[]),...(!durable?['cache:put']:[])]);calls.length=0;cases++;}
   provider={...profile.provider,object:'custom'};const key='Create'+profile.kind+'Mapper';await build({...configured,[key]:{...configured[key],validateProviderCreate:undefined}},backup)[profile.method](profile.valid);assert.equal(calls[0],'provider:create');assert.ok(calls.includes('orm:create'));calls.length=0;cases++;
  }
 }
 for(const kind of ['Plan','BillingPortal']){
  const name=kind==='Plan'?'plan':'billingPortal',configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${name}.service.ts`))['Stripe'+kind+'Service'];
  const id=randomUUID(),calls=[],row=kind==='Plan'?{id,name:'Owned',price:12,cadence:'month',currency:'USD',active:true,externalId:'plan_owned',billingProvider:'stripe'}:{id,customerId:'local_customer',uri:'https://example.test/old',expiresAt:new Date('2026-01-01T00:00:00Z'),stripeFields:{id:'bps_old'}};
  let provider;
  const em={async findOneOrFail(){calls.push('orm:read');return row;}};
  const cache={async readRecord(){calls.push('cache:read');return {value:row};}};
  const client={plans:{async retrieve(){calls.push('provider:get');return {id:'plan_owned',object:'plan',product:'prod_owned'};},async del(){calls.push('provider:delete');},async create(){calls.push('provider:create');return provider;}},billingPortal:{sessions:{async create(){calls.push('provider:create');return provider;}}}};
  const service=kind==='Plan'?new Service(client,em,{},actual.validator,configured):new Service(client,em,cache,{},actual.validator,configured);
  const expected=kind==='Plan'?['orm:read','provider:get','provider:delete','provider:create']:['cache:read','provider:create'];
  const good={id:'new_safe_id',object:kind==='Plan'?'plan':'billing_portal.session',url:'https://example.test/new'};
  for(const patch of [{id:undefined},{id:null},{id:1},{id:''},{id:' '},{id:'\ufeff'},{id:'bad\u0085id'},{id:'é'.repeat(513)},{object:'customer'}]){
   provider={...good,...patch};const before=structuredClone(row);await assert.rejects(service[kind==='Plan'?'updatePlan':'updateBillingPortalSession']({id}),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(calls,expected);assert.deepEqual(row,before);calls.length=0;cases++;
  }
 }
}
console.log(`PASS ${cases} actual configured creation-response shape cases: all five families, UTF-8/control/blank/kind bounds before local allocation/cache/persistence, effect retained, custom fallback and arbitrary safe IDs preserved`);
