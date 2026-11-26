// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');

let cases=0;
for(const variant of ['base','stripe']){
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

 for(const family of ['checkoutSession','billingPortal']){
  const name=family[0].toUpperCase()+family.slice(1),mappers=actual.load(path.join(root,`blueprint/billing-${variant}/domain/mappers/${family}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/${variant}/services/${family}.service.ts`))[(variant==='base'?'Base':'Stripe')+name+'Service'];
  for(const backup of [false,true]){
   let calls=0,writes=0,orm=0;
   const id=randomUUID(),good={id,customerId:'cus_owner',currency:'USD',paymentMethods:[variant==='base'?'credit_card':'card'],status:'PENDING',expiresAt:'2026-12-01T00:00:00Z',stripeFields:{id:'cs_fixture',object:'checkout.session',mode:'subscription',livemode:false,customer:'cus_owner'}};let value={...good};
   const cache={async readRecord(){return {value:structuredClone(value)};},async putRecord(){writes++;},getTtlMilliseconds:()=>1000};
   const em=new Proxy({},{get(){orm++;throw Error('unexpected ORM access');}});
   const client={checkout:{sessions:{async retrieve(){calls++;return good.stripeFields;}}},billingPortal:{sessions:{async create(){calls++;return {id:'bps_new'};}}}};
   const service=variant==='base'?new Service(em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup}):new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});
   const get=family==='checkoutSession'?'getCheckoutSession':'getBillingPortalSession';
   const valid=await service[get]({id});assert.equal(valid.id,id);assert.ok(valid.expiresAt instanceof Date);cases++;
   for(const requested of [id,'', ' ',undefined,null]){
    value={...good,id:requested===id?randomUUID():requested};const before=calls;
    await assert.rejects(service[get]({id:requested}));assert.equal(calls,before);assert.equal(orm,0);assert.equal(writes,0);cases++;
    if(family==='billingPortal'){await assert.rejects(service.updateBillingPortalSession({id:requested}));assert.equal(calls,before,'including backup Stripe preflight');assert.equal(orm,0);assert.equal(writes,0);cases++;}
   }
  }
 }
}
const helper=actualValidator(root).load(path.join(root,'blueprint/implementations/billing/base/utils/cachedSessionDto.ts')).cachedSessionDto;
assert.ok(helper({expiresAt:'2026-12-01'}).expiresAt instanceof Date);cases++;
for(const id of ['provider_id','not-a-uuid',' ID with spaces ']){assert.equal(helper({id,expiresAt:'2026-12-01'},id).id,id);cases++;}
console.log(`PASS ${cases} actual checkout/portal cache identity guards, JSON dates, arbitrary IDs, backup Stripe pre-provider denials and zero ORM/cache writes`);
