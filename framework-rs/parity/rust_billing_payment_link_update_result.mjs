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
  const good={id,amount:12,currency:'USD',paymentMethods:['card'],status:'PENDING',description:'preserved',stripeFields:{id:'plink_cached_reference',object:'payment_link',livemode:false},createdAt:'2026-01-01T00:00:00.000Z',updatedAt:'2026-01-02T00:00:00.000Z'};
  let value=structuredClone(good),response,posts=0,finds=0,assigns=0,flushes=0,writes=0,maps=0;
  const patch={id,stripeFields:{metadata:{note:'unchanged'}}},stripeId=good.stripeFields.id;
  const client={paymentLinks:{async update(target,fields){posts++;assert.equal(target,stripeId,'Stripe is called with the stored Stripe id, not the public id');assert.equal(JSON.stringify(fields),JSON.stringify({metadata:{note:'unchanged'}}));return response;}}};
  let entity;
  const em=new FixtureEntityManager();
  Object.assign(em,{async findOneOrFail(_entity,where){finds++;assert.equal(where.id,id);return entity;},assign(target,fields){assigns++;Object.assign(target,fields);},persist(){return {async flush(){flushes++;}};}});
  const cache={getTtlMilliseconds:()=>1000,async readRecord(){return {value:JSON.parse(JSON.stringify(value))};},async putRecord(record){writes++;value=record.value;}};
  const mappers={...configured,UpdatePaymentLinkMapper:{...configured.UpdatePaymentLinkMapper,async toEntity(...args){maps++;return configured.UpdatePaymentLinkMapper.toEntity(...args);}}};
  const service=new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});
  const invalid=[{id:'other'},{id},{id:undefined},{id:null},{id:1},{id:''},{object:'price'},{object:undefined},{line_items:{data:null}},{line_items:{data:{}}},{line_items:{data:[{}]}},{line_items:{data:[{amount_total:'1'}]}},{line_items:{data:[{amount_total:NaN}]}},{line_items:{data:[{amount_total:Infinity}]}},{line_items:{data:[{amount_total:Number.MAX_VALUE},{amount_total:Number.MAX_VALUE}]}}];
  for(const change of invalid){
   response={id:stripeId,object:'payment_link',...change};const before=structuredClone(patch),oldPosts=posts;
   await assert.rejects(service.updatePaymentLink(patch),/Invalid Stripe billing response/);
   assert.equal(posts,oldPosts+1);assert.equal(finds+assigns+flushes+writes+maps,0);assert.deepEqual(patch,before);assert.deepEqual(value,good);cases++;
  }
  for(const line_items of [undefined,{data:[]},{data:[{amount_total:12},{amount_total:3}]}]){
   value=structuredClone(good);entity={...structuredClone(good),providerFields:good.stripeFields};
   response={id:stripeId,object:'payment_link',livemode:false,line_items,metadata:{retained:'yes'}};
   const before=structuredClone(patch),oldMaps=maps,oldFinds=finds,oldWrites=writes,oldFlushes=flushes;
   const result=await service.updatePaymentLink(patch);
   assert.equal(result.id,id);assert.equal(result.amount,line_items?.data.length?15:0);assert.equal(result.description,'preserved');assert.equal(result.status,'PENDING');assert.equal(JSON.stringify(result.stripeFields),JSON.stringify(response));
   assert.equal(maps-oldMaps,backup?1:0);assert.equal(finds-oldFinds,backup?1:0);assert.equal(flushes-oldFlushes,backup?1:0);assert.equal(writes-oldWrites,1);assert.deepEqual(patch,before);cases++;
  }
  const sequence=[];
  const legacyMappers={...configured,PaymentLinkMapper:{...configured.PaymentLinkMapper,async toDto(e){sequence.push('response');return e;}},UpdatePaymentLinkMapper:{entity:configured.UpdatePaymentLinkMapper.entity,async toEntity(dto){sequence.push('entity');return {...good,...dto};}}};
  value=structuredClone(good);response={id:stripeId,object:'payment_link'};
  const legacy=new Service(client,em,cache,{},actual.validator,legacyMappers,{enableDatabaseBackup:false});
  await legacy.updatePaymentLink(patch);assert.deepEqual(sequence,['entity','response','entity','response']);cases++;
 }
}
console.log(`PASS ${cases} configured PaymentLink update result cases: stored Stripe id dispatch with the public id kept, provider id must match the stored id, numeric finite totals, one local map, cache-only zero ORM, preserved custom fallback; provider POST already occurred`);
