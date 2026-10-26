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


 const configured=actual.load(path.join(root,'blueprint/billing-stripe/domain/mappers/billingPortal.mappers.ts'));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/billingPortal.service.ts')).StripeBillingPortalService;
 for(const backup of [false,true]) for(const mode of ['create','replace']) {
  const id=randomUUID(),calls=[];
  const old={id,customerId:'cus_owned',uri:'https://example.test/old',expiresAt:new Date('2026-01-01T00:00:00Z'),createdAt:new Date('2025-01-01T00:00:00Z'),updatedAt:new Date('2025-01-01T00:00:00Z'),stripeFields:{id:'bps_old'}};
  let value=structuredClone(old),entity,response;
  const em=new FixtureEntityManager();Object.assign(em,{
   create(_entity,fields){calls.push('orm:create');return {id,...fields};},
   async findOneOrFail(){calls.push('orm:read');return entity;},
   assign(target,fields){calls.push('orm:assign');Object.assign(target,fields);},
   persist(){calls.push('orm:persist');return {async flush(){calls.push('orm:flush');}};}
  });
  const cache={async readRecord(){calls.push('cache:read');return {value:structuredClone(value)};},async putRecord(record){calls.push('cache:put');value=record.value;},getTtlMilliseconds:()=>1000};
  const client={billingPortal:{sessions:{async create(fields){calls.push('provider:create');assert.equal(fields.customer,'cus_owned');return response;}}}};
  const build=(mappers=configured)=>new Service(client,em,cache,{},actual.validator,mappers,{enableDatabaseBackup:backup});
  const input=mode==='create'?{customerId:'cus_owned'}:{id};
  const invoke=service=>service[mode==='create'?'createBillingPortalSession':'updateBillingPortalSession'](input);
  for(const url of [undefined,null,1,true,{},[], '', '   ','\t\n','\ufeff']){
   response={id:'bps_new',object:'billing_portal.session',url};const before=structuredClone(input);
   await assert.rejects(invoke(build()),error=>error.message==='Invalid Stripe billing response');
   assert.deepEqual(calls,mode==='create'?['provider:create']:['cache:read','provider:create']);
   assert.deepEqual(value,old);assert.deepEqual(input,before);calls.length=0;cases++;
  }
  for(const url of ['https://example.test/new?secret=kept','custom:verbatim',' relative-preserved ']){
   value=structuredClone(old);entity={...structuredClone(old),providerFields:old.stripeFields};
   response={id:'bps_new',object:'billing_portal.session',url};const before=structuredClone(input);
   const result=await invoke(build());assert.equal(result.uri,url);assert.equal(result.stripeFields.id,'bps_new');assert.equal(result.id,id);assert.equal(value.uri,url);assert.equal(value.stripeFields.id,'bps_new');assert.deepEqual(input,before);
   assert.equal(calls.filter(x=>x==='provider:create').length,1);assert.equal(calls.filter(x=>x==='cache:put').length,1);calls.length=0;cases++;
  }
  // The optional shape hook alone is removed: custom mappers keep their existing behavior.
  value=structuredClone(old);entity={...structuredClone(old),providerFields:old.stripeFields};
  response={id:'bps_new',object:'billing_portal.session'};
  await invoke(build({...configured,CreateBillingPortalMapper:{...configured.CreateBillingPortalMapper,validateProviderCreate:undefined}}));
  assert.equal(calls.filter(x=>x==='provider:create').length,1);assert.ok(calls.includes('cache:put'));cases++;
 }
}
console.log(`PASS ${cases} configured portal URL shape cases: create/replacement, backup/cache, required nonblank string including BOM, one already-applied POST and zero local writes on rejection, verbatim valid URL, custom fallback`);
