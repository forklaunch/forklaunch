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

 const configured=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/paymentLink.mappers.ts'));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/paymentLink.service.ts')).StripePaymentLinkService;
 for(const backup of [false,true]) {
  const id=randomUUID();
  const good={id,amount:12,currency:'USD',paymentMethods:['card'],status:'PENDING',stripeFields:{id:'plink_cached_reference',object:'payment_link',livemode:false,nested:{date:'2026-01-01'}},createdAt:'2026-01-01T00:00:00.000Z',updatedAt:'2026-01-02T00:00:00.000Z'};
  let value=structuredClone(good),provider=0,orm=0,writes=0,normalized=0,hookCalls=0;
  const stop=new Error('owned provider seam');
  const client={paymentLinks:{async update(target,fields){provider++;assert.equal(target,'plink_cached_reference','the stored Stripe id, not the public id, is the dispatch target');assert.equal(JSON.stringify(fields),JSON.stringify({metadata:{note:'unchanged'}}));throw stop;}}};
  const em=new Proxy({}, {get(){orm++;throw new Error('unexpected ORM operation');}});
  const cache={getTtlMilliseconds:()=>1000,async readRecord(){return value?{value:JSON.parse(JSON.stringify(value))}:undefined;},async putRecord(){writes++;}};
  const mappers={...configured,PaymentLinkMapper:{...configured.PaymentLinkMapper,normalizeCachedDto(value){normalized++;return configured.PaymentLinkMapper.normalizeCachedDto(value);}},UpdatePaymentLinkMapper:{...configured.UpdatePaymentLinkMapper,async validateCachedUpdate(dto,existing){hookCalls++;assert.ok(existing.createdAt instanceof Date);assert.equal(existing.stripeFields.nested.date,'2026-01-01');const before=JSON.stringify(existing),created=existing.createdAt;await configured.UpdatePaymentLinkMapper.validateCachedUpdate(dto,existing);assert.equal(JSON.stringify(existing),before);assert.equal(existing.createdAt,created);}}};
  const service=new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});
  for(const patch of [{id,amount:'invalid'},{id,status:'INVALID'},{id,currency:'INVALID'},{id,paymentMethods:['invalid']},{id:''},{id:randomUUID()}]) {
   const before=structuredClone(patch);await assert.rejects(service.updatePaymentLink(patch));assert.deepEqual(patch,before);assert.deepEqual(value,good);assert.equal(provider,0);assert.equal(orm,0);assert.equal(writes,0);cases++;
  }
  for(const change of [{id:randomUUID()},{id:' '},{amount:'invalid'},{status:'INVALID'},{createdAt:'bad-date'},{updatedAt:'bad-date'}]) {
   value={...good,...change};const before=structuredClone(value);await assert.rejects(service.updatePaymentLink({id,amount:34}));assert.deepEqual(value,before);assert.equal(provider,0);assert.equal(orm,0);assert.equal(writes,0);cases++;
  }
  value=undefined;await assert.rejects(service.updatePaymentLink({id,amount:34}));assert.equal(provider,0);assert.equal(orm,0);assert.equal(writes,0);cases++;
  value=structuredClone(good);const patch={id,stripeFields:{metadata:{note:'unchanged'}}},original=structuredClone(patch);normalized=0;hookCalls=0;
  await assert.rejects(service.updatePaymentLink(patch),error=>error===stop);assert.equal(normalized,1);assert.equal(hookCalls,1);assert.equal(provider,1);assert.equal(orm,0);assert.equal(writes,0);assert.deepEqual(value,good);assert.deepEqual(patch,original);cases++;
  // A cached record without a stored Stripe id fails before any provider call.
  const nested={date:'2026-01-01'};
  for(const stripeFields of [{object:'payment_link',nested},{id,object:'payment_link',nested},{id:'cs_wrong_kind',nested},{id:'plink_',nested},{id:7,nested}]){
   value={...structuredClone(good),stripeFields};
   await assert.rejects(service.updatePaymentLink(structuredClone(patch)),{message:`Payment link ${id} has no stored Stripe payment link id`});assert.equal(provider,1);assert.equal(orm,0);assert.equal(writes,0);cases++;
  }
  value=structuredClone(good);
  // Without the configured hook the record is still read once, for its stored Stripe id only.
  let legacyReads=0;
  const legacy=new Service(client,em,{getTtlMilliseconds:()=>1000,async readRecord(){legacyReads++;return{value:JSON.parse(JSON.stringify(good))};},async putRecord(){writes++;}}, {},actual.validator,{...mappers,UpdatePaymentLinkMapper:{...mappers.UpdatePaymentLinkMapper,validateCachedUpdate:undefined}},{enableDatabaseBackup:backup});
  await assert.rejects(legacy.updatePaymentLink(patch),error=>error===stop);assert.equal(legacyReads,1);assert.equal(provider,2);assert.equal(orm,0);assert.equal(writes,0);cases++;
 }
}
console.log(`PASS ${cases} actual configured Stripe PaymentLink update preflight cases: zero side effects on invalid input/cache or a missing stored Stripe id, one normalization, stored Stripe id dispatch with unchanged public id/input, backup and custom fallback`);
