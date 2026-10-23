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

 const mappers=actual.load(path.join(root,`blueprint/billing-${variant}/domain/mappers/subscription.mappers.ts`));
 const Service=actual.load(path.join(root,'blueprint/implementations/billing/base/services/subscription.service.ts')).BaseSubscriptionService;
 const rows=[],queries=[];
 const em={create(_e,value){const row={id:randomUUID(),...value};rows.push(row);return row;},async findOneOrFail(_e,query){queries.push(query);const row=rows.find(row=>Object.entries(query).every(([key,value])=>row[key]===value));if(!row)throw Error('not found');return row;}};
 const service=new Service(em,{},actual.validator,mappers);
 for(const kind of ['user','organization']){
  const input={partyId:'same-party',partyType:kind,productId:'product',active:true,externalId:'sub_'+kind,startDate:new Date('2026-01-01'),status:'active',billingProvider:'stripe'};
  const parsed=actual.validator.parse(actual.validator.schemify(mappers.CreateSubscriptionMapper.schema),input);assert.equal(parsed.ok,true);
  await mappers.CreateSubscriptionMapper.toEntity({...parsed.value,active:false},em,{id:'sub_inactive'});
  const row=await mappers.CreateSubscriptionMapper.toEntity(parsed.value,em,{id:'sub_'+kind});
  const method=kind==='user'?'getUserSubscription':'getOrganizationSubscription';
  const result=await service[method]({id:'same-party'});assert.equal(result.id,row.id);assert.equal(result.partyType,kind);assert.equal(result.active,true);cases++;
  await assert.rejects(service[method]({id:'absent'}));cases++;
 }
 for(const invalid of [null,[],{}, {user:'',organization:'org'},{user:' ',organization:'org'},{user:'user',organization:4},{user:'same',organization:'same'}]){assert.throws(()=>new Service(em,{},actual.validator,{...mappers,SubscriptionMapper:{...mappers.SubscriptionMapper,partyTypes:invalid}}));cases++;}
 const mutable={user:'user',organization:'organization'},copy=new Service(em,{},actual.validator,{...mappers,SubscriptionMapper:{...mappers.SubscriptionMapper,partyTypes:mutable}});mutable.user='redirected';assert.equal((await copy.getUserSubscription({id:'same-party'})).partyType,'user');cases++;
 const legacy=new Service(em,{},actual.validator,{...mappers,SubscriptionMapper:{...mappers.SubscriptionMapper,partyTypes:undefined,toDto:async row=>row}});
 rows.push({id:'legacy-user',partyId:'legacy',partyType:'USER',active:true},{id:'legacy-org',partyId:'legacy',partyType:'ORGANIZATION',active:true});
 assert.equal((await legacy.getUserSubscription({id:'legacy'})).id,'legacy-user');assert.equal((await legacy.getOrganizationSubscription({id:'legacy'})).id,'legacy-org');cases+=2;
 assert.ok(queries.every(query=>query.active===true));
}
console.log(`PASS ${cases} actual configured subscription party lookups: enum persistence, active/kind isolation, missing rows, metadata validation, immutable capture and legacy fallback`);
