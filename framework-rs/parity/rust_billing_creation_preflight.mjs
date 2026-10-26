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
 for(const profile of profiles){
  const configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${profile.name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${profile.name}.service.ts`))['Stripe'+profile.kind+'Service'];
  const calls=[],id=randomUUID();let sent;
  const em=new FixtureEntityManager();Object.assign(em,{create(_entity,fields){calls.push('orm:create');return {id,...fields};},async transactional(fn){calls.push('orm:transaction');return fn(em);},persist(){calls.push('orm:persist');return {async flush(){calls.push('orm:flush');}};}});
  const cache={async putRecord(record){calls.push('cache:put');assert.equal(record.value.id,id);},getTtlMilliseconds(){return 60000;}};
  const create=async options=>{calls.push('stripe:create');sent=structuredClone(options);return profile.provider;};
  const client={subscriptions:{create},checkout:{sessions:{create}},billingPortal:{sessions:{create}},paymentLinks:{create}};
  function build(mappers= configured,backup=false){return profile.name==='subscription'?new Service(client,em,{},actual.validator,mappers):new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});}
  const service=build();
  for(const patch of profile.patches){
   const input={...profile.valid,...patch},before=structuredClone(input);
   await assert.rejects(service[profile.method](input),new RegExp('Invalid '+profile.name+' creation'),JSON.stringify(patch));
   assert.deepEqual(input,before);assert.deepEqual(calls,[],profile.name+JSON.stringify(patch));cases++;
  }
  const variants=[{},profile.overrides];
  if(profile.name!=='paymentLink')variants.push({stripeFields:undefined});
  if(profile.name==='paymentLink')variants.push({amount:undefined},{amount:null});
  for(const backup of profile.name==='subscription'?[false]:[false,true])for(const overrides of variants){
   const input={...profile.valid,...overrides},before=structuredClone(input);
   const result=await build(configured,backup)[profile.method](input);
   const expected=profile.name==='subscription'?['stripe:create','orm:create','orm:transaction','orm:persist']:['stripe:create','orm:create',...(backup?['orm:persist','orm:flush']:[]),'cache:put'];
   assert.deepEqual(calls,expected,profile.name);assert.deepEqual(input,before);assert.equal(result.id,id);assert.deepEqual(result.stripeFields,profile.provider);
   if(profile.name==='subscription'){assert.equal(result.externalId,'sub_owned');assert.equal(result.billingProvider,'stripe');assert.deepEqual(sent,{...input.stripeFields,customer:'cus_owned',items:[{plan:'plan_owned'}]});}
   if(profile.name==='checkoutSession'){assert.equal(result.uri,profile.provider.url);assert.ok(Number.isFinite(result.expiresAt.getTime()));assert.deepEqual(sent,{...input.stripeFields,mode:'subscription',payment_method_types:['card'],currency:'USD',success_url:undefined,cancel_url:undefined});}
   if(profile.name==='billingPortal'){assert.equal(result.uri,profile.provider.url);assert.ok(Number.isFinite(result.expiresAt.getTime()));assert.deepEqual(sent,{...input.stripeFields,customer:'cus_owned'});}
   if(profile.name==='paymentLink'){assert.equal(result.amount,12);assert.deepEqual(sent,{...profile.valid.stripeFields,payment_method_types:['card'],currency:'USD'});}
   calls.length=0;cases++;
  }
  const stop=new Error('owned fallback seam');
  client.subscriptions.create=client.checkout.sessions.create=client.billingPortal.sessions.create=client.paymentLinks.create=async()=>{calls.push('stripe:create');throw stop;};
  const key='Create'+profile.kind+'Mapper';
  const fallback=build({...configured,[key]:{...configured[key],validateCreate:undefined}});
  await assert.rejects(fallback[profile.method]({...profile.valid,...profile.patches[0]}),error=>error===stop);assert.deepEqual(calls,['stripe:create']);cases++;
 }
}
console.log(`PASS ${cases} actual configured Stripe creation preflight cases: invalid fields have zero provider/ORM/cache effects; valid cache-only/backup sequence, provider-owned overrides, nullish amount derivation, input immutability and custom fallback preserved`);
