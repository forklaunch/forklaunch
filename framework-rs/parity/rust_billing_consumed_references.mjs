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

 for(const kind of ['Plan','Subscription']){
  const name=kind[0].toLowerCase()+kind.slice(1),configured=actual.load(path.join(root,`blueprint/billing-stripe/domain/mappers/${name}.mappers.ts`));
  const Service=actual.load(path.join(root,`blueprint/implementations/billing/stripe/services/${name}.service.ts`))['Stripe'+kind+'Service'];
  const id=randomUUID(),externalId=name+'_owned',calls=[];
  const initial=kind==='Plan'?{id,externalId,name:'Owned',price:12,cadence:'month',currency:'USD',active:true,billingProvider:'stripe',providerFields:{id:'prod_old'}}:{id,externalId,partyId:'local_customer',partyType:'user',productId:'local_product',active:true,billingProvider:'stripe',startDate:new Date('2026-01-01T00:00:00Z'),status:'ACTIVE',providerFields:{id:'sub_old'}};
  let row=structuredClone(initial),provider;
  const good={id:externalId,object:name,product:'prod_owned',interval:'month',currency:'USD',amount:12,items:{data:[{id:'si_owned',plan:{id:'plan_owned'}}]}};
  const updated={id:kind==='Plan'?'plan_new':externalId,object:name,product:{id:'prod_owned',object:'product'},customer:'cus_distinct_from_local_uuid'};
  const api={async retrieve(target){calls.push('provider:get:'+target);return target==='plan_new'?updated:provider;},async del(target){calls.push('provider:delete:'+target);},async create(fields){calls.push('provider:create');assert.equal(fields.product,'prod_owned');assert.equal(fields.amount,12);return updated;},async update(target,fields){calls.push('provider:update:'+target);assert.deepEqual(structuredClone(fields),{items:[{id:'si_owned',plan:'plan_owned'}]});return updated;}};
  const client={[name==='plan'?'plans':'subscriptions']:api};
  const em={async findOneOrFail(){calls.push('orm:read');return row;},assign(entity,fields){calls.push('orm:assign');Object.assign(entity,fields);},async upsert(entity){calls.push('orm:upsert');return entity;},async transactional(fn){calls.push('orm:transaction');return fn(em);},async persist(){calls.push('orm:persist');}};
  const service=new Service(client,em,{},actual.validator,configured),method='update'+kind;

  const badIds=[undefined,null,1,{},[], '', ' ','\ufeff','x\u0000y','x\u0085y','x'.repeat(1025),'é'.repeat(513)];
  const invalid=kind==='Plan'?[undefined,null,1,[],{},...badIds,...badIds.map(id=>({id}))].map(product=>({...good,product})):[undefined,null,{},'invalid',...[null,{},...badIds.map(id=>({id})),...badIds.filter(id=>id!==undefined).map(id=>({id:'si_owned',plan:{id}}))].map(item=>({data:[item]}))].map(items=>({...good,items}));
  for(const bad of invalid){provider=bad;const input={id},before=structuredClone(input);await assert.rejects(service[method](input),error=>error.message==='Invalid Stripe billing response');assert.deepEqual(row,initial);assert.deepEqual(input,before);assert.deepEqual(calls,['orm:read','provider:get:'+externalId]);calls.length=0;cases++;}
  // Inspect the exact next mutation payload without allowing any local mutation.
  const reached=new Error('owned mutation seam');
  if(kind==='Plan'){
   for(const product of ['prod_owned',{id:'prod_owned'},'x'.repeat(1024),'é'.repeat(512)]){
    provider={...good,product};const wanted=typeof product==='string'?product:product.id;
    api.create=async fields=>{calls.push('provider:create');assert.equal(fields.product,wanted);throw reached;};
    await assert.rejects(service[method]({id}),error=>error===reached);assert.deepEqual(calls,['orm:read','provider:get:'+externalId,'provider:delete:'+externalId,'provider:create']);calls.length=0;cases++;
   }
   provider={...good,product:{}};
   const legacy=new Service(client,em,{},actual.validator,{...configured,UpdatePlanMapper:{...configured.UpdatePlanMapper,validateProviderUpdate:undefined}});
   api.create=async fields=>{calls.push('provider:create');assert.equal(fields.product,undefined);throw reached;};
   await assert.rejects(legacy[method]({id}),error=>error===reached);assert.deepEqual(calls,['orm:read','provider:get:'+externalId,'provider:delete:'+externalId,'provider:create']);calls.length=0;cases++;
  } else {
   const profiles=[
    {items:[],dto:{},expected:[]},
    {items:[{id:'si_owned'}],dto:{},expected:[{id:'si_owned'}]},
    {items:[{id:'si_owned',plan:null}],dto:{},expected:[{id:'si_owned'}]},
    {items:[{id:'si_owned',plan:{}}],dto:{},expected:[{id:'si_owned'}]},
    {items:[{id:'si_owned',plan:{id:'plan_owned'}}],dto:{productId:''},expected:[{id:'si_owned',plan:'plan_owned'}]},
    ...badIds.map(id=>({items:[{id:'si_owned',plan:{id}}],dto:{productId:'caller_override'},expected:[{id:'si_owned',plan:'caller_override'}]}))
   ];
   for(const profile of profiles){provider={...good,items:{data:profile.items}};api.update=async(target,fields)=>{calls.push('provider:update:'+target);assert.equal(JSON.stringify(fields),JSON.stringify({items:profile.expected}));throw reached;};const input={id,...profile.dto},before=structuredClone(input);await assert.rejects(service[method](input),error=>error===reached);assert.deepEqual(input,before);assert.deepEqual(calls,['orm:read','provider:get:'+externalId,'provider:update:'+externalId]);assert.deepEqual(row,initial);calls.length=0;cases++;}
   provider={...good,items:{data:[{id:null}]}};
   const legacy=new Service(client,em,{},actual.validator,{...configured,UpdateSubscriptionMapper:{...configured.UpdateSubscriptionMapper,validateProviderUpdate:undefined}});
   api.update=async(target,fields)=>{calls.push('provider:update:'+target);assert.equal(fields.items[0].id,null);throw reached;};
   await assert.rejects(legacy[method]({id}),error=>error===reached);assert.deepEqual(calls,['orm:read','provider:get:'+externalId,'provider:update:'+externalId]);calls.length=0;cases++;
  }
 }
}
console.log(`PASS ${cases} configured consumed provider reference cases: safe IDs before Plan DELETE/Subscription POST, absent fallback preserved, truthy caller override ignores unused fallback, exact custom fallback and no local writes`);
