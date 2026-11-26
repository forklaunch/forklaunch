// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');

let cases=0;
for(const variant of ['stripe']){
 let actual;
 class FixtureEntityManager {}
 const entities=new Map();
 actual=actualValidator(root,(spec,file,load)=>{
  if(spec==='@forklaunch/validator/zod')return actual.statics;
  if(spec==='@forklaunch/blueprint-core')return {schemaValidator:actual.validator};
  if(spec==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{}})};
  if(spec==='@forklaunch/core/cache')return {createCacheKey:prefix=>id=>prefix+':'+id};
  if(spec==='@forklaunch/implementation-billing-base/services')return {...load(path.join(root,'blueprint/implementations/billing/base/services/billingPortal.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/checkoutSession.service.ts')),...load(path.join(root,'blueprint/implementations/billing/base/services/paymentLink.service.ts'))};
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

 const samples=JSON.parse(await (await import('node:fs/promises')).readFile(path.join(root,'framework-rs/parity/fixtures/rust_billing_cached_dates.json'),'utf8'));
 for(const [family,name,fields] of [['checkoutSession','CheckoutSession',['expiresAt','createdAt','updatedAt']],['paymentLink','PaymentLink',['createdAt','updatedAt']]]) {
  const mappers=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${family}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${family}.service.ts`))[`Stripe${name}Service`];
  const provider=family==='checkoutSession'?{id:'cs_owned',object:'checkout.session',mode:'subscription',livemode:false,customer:null}:{id:'plink_owned',object:'payment_link',livemode:false};
  const original={id:'local-id',amount:12,customerId:'local-customer',currency:'USD',paymentMethods:['card'],status:'PENDING',expiresAt:'2026-01-01T00:00:00Z',stripeFields:provider};
  let encoded,calls=0,live;
  const cache={async readRecord(){return {value:live??JSON.parse(encoded)};},async putRecord(){throw Error('GET wrote cache');}};
  const retrieve=async()=>{calls++;return {...provider,metadata:{opaqueDate:'not-a-date'}};};
  const service=new Service({checkout:{sessions:{retrieve}},paymentLinks:{retrieve}},{},cache,{},actual.validator,mappers);
  for(const field of fields)for(const item of samples) {
   encoded=JSON.stringify({...original,[field]:item.input});const before=encoded;calls=0;
   if(item.profile==='invalid') {await assert.rejects(service[`get${name}`]({id:original.id}),undefined,`${family}/${field}/${item.name}`);assert.equal(calls,0);}
   else {const result=await service[`get${name}`]({id:original.id});assert.ok(result[field] instanceof Date);assert.equal(result[field].getTime(),item.epochMillis??new Date(item.input).getTime(),`${family}/${field}/${item.name}`);assert.deepEqual(result.stripeFields.metadata,{opaqueDate:'not-a-date'});assert.equal(calls,1);}
   assert.equal(encoded,before);cases++;
  }
  // Date objects and non-finite numbers exist in an in-process cache, not JSON.
  for(const field of fields)for(const value of [new Date(0),NaN,Infinity,-Infinity]) {
   live={...original,[field]:value};calls=0;
   if(value instanceof Date){assert.equal((await service[`get${name}`]({id:original.id}))[field].getTime(),0);assert.equal(calls,1);assert.equal(value.getTime(),0);}
   else {await assert.rejects(service[`get${name}`]({id:original.id}));assert.equal(calls,0);}
   cases++;live=undefined;
  }
  for(const field of fields) {const dto={...original};delete dto[field];encoded=JSON.stringify(dto);calls=0;if(field==='expiresAt'){await assert.rejects(service[`get${name}`]({id:original.id}));assert.equal(calls,0);}else{assert.equal((await service[`get${name}`]({id:original.id}))[field],undefined);assert.equal(calls,1);}cases++;}
 }
}
console.log(`PASS ${cases} actual configured Checkout/PaymentLink cached-date cases: distinct base/Zod paths, accepted epoch/string/extended years, optional omission, invalid null/type/date zero provider calls; locale/offsetless/chrono-outside cases accepted by JS but explicitly outside native subset. JSON cache immutable, opaque provider dates unchanged.`);
