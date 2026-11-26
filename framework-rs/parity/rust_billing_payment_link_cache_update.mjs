// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');

let cases=0;
{ const variant='base';
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

 const mappers=actual.load(path.join(root,'blueprint/billing-base/domain/mappers/paymentLink.mappers.ts'));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/base/services/paymentLink.service.ts')).BasePaymentLinkService;
 for(const backup of [false,true]) {
  let reads=0,persists=0,writes=0;const stored=new Map();let value;
  const em={create(_e,fields){const item={id:randomUUID(),createdAt:new Date('2026-01-01'),updatedAt:new Date('2026-01-02'),...fields};stored.set(item.id,item);return item;},async findOneOrFail(_e,{id}){reads++;assert.ok(backup,'cache-only must not query ORM');return stored.get(id);},assign(item,fields){Object.assign(item,fields);},persist(){persists++;return {async flush(){}};}};
  const cache={getTtlMilliseconds:()=>1000,async readRecord(){return value?{value:JSON.parse(JSON.stringify(value))}:undefined;},async putRecord(record){writes++;value=JSON.parse(JSON.stringify(record.value));}};
  const service=new Service(em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});
  const created=await service.createPaymentLink({amount:12,currency:'USD',paymentMethods:['credit_card'],status:'PENDING',providerFields:{nested:{date:'2026-01-01'}}});
  const result=await service.updatePaymentLink({id:created.id,amount:34});
  assert.equal(result.amount,34);assert.equal(result.id,created.id);assert.equal(reads,backup?1:0);assert.equal(persists,backup?2:0);assert.ok(result.createdAt instanceof Date);assert.equal(result.providerFields.nested.date,'2026-01-01');cases++;
  if(!backup){
   const good=structuredClone(value);
   for(const patch of [{id:randomUUID(),amount:1},{id:created.id,amount:'invalid'},{id:created.id,status:'INVALID'}]){const before=writes;await assert.rejects(service.updatePaymentLink(patch));assert.equal(writes,before);assert.deepEqual(value,good);cases++;}
   for(const change of [{id:randomUUID()},{amount:'invalid'},{createdAt:'not-a-date'}]){value={...good,...change};const before=writes;await assert.rejects(service.updatePaymentLink({id:created.id,amount:35}));assert.equal(writes,before);cases++;}
   value=undefined;const missingWrites=writes;await assert.rejects(service.updatePaymentLink({id:created.id,amount:35}));assert.equal(writes,missingWrites);cases++;
   value=good;
   const manager=new FixtureEntityManager(),context={request:'fixture'};
   const forwarded=new Service(em,cache,{},actual.validator,{...mappers,UpdatePaymentLinkMapper:{...mappers.UpdatePaymentLinkMapper,async toCachedDto(input,existing,selected,...args){assert.equal(selected,manager);assert.deepEqual(args,[context]);return mappers.UpdatePaymentLinkMapper.toCachedDto(input,existing,selected);}}});
   await forwarded.updatePaymentLink({id:created.id,amount:34},manager,context);cases++;
   const malicious=new Service(em,cache,{},actual.validator,{...mappers,UpdatePaymentLinkMapper:{...mappers.UpdatePaymentLinkMapper,toCachedDto:async()=>({...good,id:randomUUID()})}});
   const before=writes;await assert.rejects(malicious.updatePaymentLink({id:created.id,amount:35}));assert.equal(writes,before);cases++;
   let called=0;const legacy=new Service(em,cache,{},actual.validator,{...mappers,UpdatePaymentLinkMapper:{...mappers.UpdatePaymentLinkMapper,toCachedDto:undefined,async toEntity(){called++;return {...good,amount:56};}}});
   assert.equal((await legacy.updatePaymentLink({id:created.id,amount:56})).amount,56);assert.equal(called,1);cases++;
  }
 }
}
console.log(`PASS ${cases} actual configured base PaymentLink cache-only updates, identity/validation denials, JSON dates, backup and custom fallback`);
