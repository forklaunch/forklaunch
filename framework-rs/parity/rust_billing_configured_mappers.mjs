// Runs the configured application mappers and their real Zod schema factories.
// ORM entity allocation/storage is owned by this fixture; no ORM decorator emulation.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const families=['plan','subscription','checkoutSession','paymentLink','billingPortal'];
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
 const samples={
  plan:{name:'Configured plan',price:1200,cadence:variant==='base'?'MONTHLY':'month',currency:'USD',externalId:'prod_fixture',billingProvider:'stripe',active:true},
  subscription:{partyId:'customer',partyType:'user',productId:'product',active:true,externalId:'sub_fixture',startDate:new Date('2026-01-01T00:00:00Z'),status:'active',billingProvider:'stripe'},
  checkoutSession:{customerId:'customer',currency:'USD',paymentMethods:[variant==='base'?'credit_card':'card'],uri:'https://fixture.test/checkout',expiresAt:new Date('2026-12-01T00:00:00Z'),status:'PENDING'},
  paymentLink:{amount:1200,currency:'USD',paymentMethods:[variant==='base'?'credit_card':'card'],status:'PENDING',...(variant==='stripe'?{stripeFields:{line_items:[{price:'price_fixture',quantity:1}]}}:{})},
  billingPortal:{customerId:'customer',expiresAt:new Date('2026-12-01T00:00:00Z'),uri:'https://fixture.test/portal'}
 };
 for(const family of families){
  const name=family[0].toUpperCase()+family.slice(1),mappers=actual.load(path.join(root,`blueprint/billing-${variant}/domain/mappers/${family}.mappers.ts`));
  const create=mappers[`Create${name}Mapper`],update=mappers[`Update${name}Mapper`],read=mappers[`${name}Mapper`];
  const records=new Map(),em={
   create(_entity,fields){const entity={id:randomUUID(),...fields};records.set(entity.id,entity);return entity;},
   async findOneOrFail(_entity,{id}){assert.ok(records.has(id),'configured update requires persisted entity');return records.get(id);},
   assign(entity,fields){Object.assign(entity,fields);}
  };
  function parse(mapper,dto){const parsed=actual.validator.parse(actual.validator.schemify(mapper.schema),dto);assert.equal(parsed.ok,true,JSON.stringify(parsed.errors));return parsed.value;}
  const input=parse(create,{...samples[family],undeclaredField:'strip-at-http-boundary'});
  assert.equal(input.undeclaredField,undefined);cases++;
  // Provider fields are optional in the actual runtime validator, even for Stripe.
  const entity=await create.toEntity(input,em,variant==='stripe'?{id:'provider_fixture'}:undefined);
  assert.match(entity.id,/^[a-f0-9-]{36}$/);assert.equal(entity.undeclaredField,undefined);
  assert.equal(entity.customerId??entity.partyId??entity.name,samples[family].customerId??samples[family].partyId??samples[family].name);cases++;
  const dto=await read.toDto(entity);
  if(variant==='stripe')assert.equal(dto.stripeFields.id,'provider_fixture');
  if(family==='plan')assert.equal(dto.price,1200);
  if(family==='paymentLink')assert.equal(dto.amount,1200);cases++;
  if(variant==='stripe'&&family==='paymentLink'){
   const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/paymentLink.service.ts')).StripePaymentLinkService;
   const provider={id:'plink_owned',object:'payment_link',livemode:false,active:true},fresh={...provider,active:false};
   const original={...dto,stripeFields:provider};let cached=JSON.stringify(original),calls=[],response=fresh,writes=0;
   const cache={async readRecord(){return cached?{value:JSON.parse(cached)}:null;},async putRecord(){writes++;}};
   const client={paymentLinks:{async retrieve(id){calls.push(id);return response;}}};
   const service=new Service(client,{},cache,{},actual.validator,mappers);
   const got=await service.getPaymentLink({id:dto.id});assert.equal(got.id,dto.id);assert.equal(got.amount,dto.amount);assert.equal(got.stripeFields,fresh);assert.deepEqual(calls,['plink_owned']);assert.equal(cached,JSON.stringify(original));assert.equal(writes,0);cases++;
   for(const patch of [{id:randomUUID()},{id:null},{status:'unknown'},{stripeFields:null},{stripeFields:[]},{stripeFields:{...provider,id:'public-id'}},{stripeFields:{...provider,id:'plink_'+ 'x'.repeat(128)}},{stripeFields:{...provider,object:'checkout.session'}},{stripeFields:{...provider,livemode:'false'}}]){
    cached=JSON.stringify({...original,...patch});calls=[];await assert.rejects(service.getPaymentLink({id:dto.id}));assert.deepEqual(calls,[]);assert.equal(writes,0);cases++;
   }
   cached=null;calls=[];await assert.rejects(service.getPaymentLink({id:dto.id}));assert.deepEqual(calls,[]);cases++;
   cached=JSON.stringify(original);
   for(const bad of [null,[],{...fresh,id:'plink_other'},{...fresh,livemode:true},{...fresh,object:'customer'},{...fresh,livemode:null}]){
    calls=[];response=bad;await assert.rejects(service.getPaymentLink({id:dto.id}));assert.deepEqual(calls,['plink_owned']);assert.equal(cached,JSON.stringify(original));assert.equal(writes,0);cases++;
   }
   const failed=new Service({paymentLinks:{async retrieve(){throw Error('provider unavailable');}}},{},cache,{},actual.validator,mappers);
   await assert.rejects(failed.getPaymentLink({id:dto.id}),/provider unavailable/);assert.equal(cached,JSON.stringify(original));assert.equal(writes,0);cases++;
   response=fresh;calls=[];const backed=new Service(client,{},cache,{},actual.validator,mappers,{enableDatabaseBackup:true});
   assert.equal((await backed.getPaymentLink({id:dto.id})).id,dto.id);assert.deepEqual(calls,['plink_owned']);assert.equal(writes,0);cases++;
   // Custom mappers retain the existing provider-ID-first behavior.
   calls=[];response=fresh;const legacy=new Service(client,{},cache,{},actual.validator,{...mappers,PaymentLinkMapper:{...read,readCachedDto:undefined}});
   assert.equal((await legacy.getPaymentLink({id:dto.id})).id,dto.id);assert.deepEqual(calls,[dto.id]);cases++;
  }
  if(variant==='stripe'&&family==='paymentLink'){
   const Service=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/paymentLink.service.ts')).StripePaymentLinkService;
   const values=Array.from({length:150},(_,i)=>({...dto,id:randomUUID(),stripeFields:{id:`plink_${i}`,object:'payment_link',livemode:false,active:true}}));
   const rows=new Map(values.map(v=>['payment_link:'+v.id,JSON.stringify(v)]));
   let reads=0,gets=[],lists=0,writes=0,inflight=0,peak=0,change=x=>x;
   const cache={async listKeys(){return [...rows.keys()];},async readRecord(key){reads++;return rows.has(key)?{value:JSON.parse(rows.get(key))}:null;},async putRecord(){writes++;}};
   const client={paymentLinks:{async retrieve(id){gets.push(id);inflight++;peak=Math.max(peak,inflight);await Promise.resolve();inflight--;return change({id,object:'payment_link',livemode:false,active:true});},async list(){lists++;return {data:values.map(v=>({id:v.id,active:true}))};}}};
   const telemetry={log(){}};
   for(const limit of [0,-1,1001,1.5,NaN,Infinity,null]){assert.throws(()=>new Service(client,{},cache,telemetry,actual.validator,mappers,{maxConfiguredListRecords:limit}),/limit/);cases++;}
   const normal=new Service(client,{},cache,telemetry,actual.validator,mappers);
   for(const request of [undefined,{ids:[]},{ids:values.slice(0,101).map(v=>v.id)}]){
    await assert.rejects(normal.listPaymentLinks(request),/limit/);assert.equal(reads,0);assert.equal(gets.length,0);assert.equal(lists,0);cases++;
   }
   for(const id of [null,7,'','   ']){await assert.rejects(normal.listPaymentLinks({ids:[id]}),/identity/);assert.equal(reads,0);assert.equal(gets.length,0);cases++;}
   const configured=new Service(client,{},cache,telemetry,actual.validator,mappers,{maxConfiguredListRecords:150});
   assert.deepEqual(Array.from(await configured.listPaymentLinks(),v=>v.id),values.map(v=>v.id));assert.equal(gets.length,150);assert.equal(reads,150);assert.equal(peak,1);assert.equal(lists,0);assert.equal(writes,0);cases++;
   const selected=[values[1].id,values[0].id,values[1].id];gets=[];
   assert.deepEqual(Array.from(await normal.listPaymentLinks({ids:selected}),v=>v.id),selected);assert.deepEqual(gets,['plink_1','plink_0','plink_1']);cases++;
   const key='payment_link:'+values[1].id,original=rows.get(key);
   for(const bad of [null,{...values[1],id:randomUUID()},{...values[1],status:'unknown'},{...values[1],stripeFields:{id:'plink_1',object:'checkout.session',livemode:false}}]){
    if(bad===null)rows.delete(key);else rows.set(key,JSON.stringify(bad));gets=[];
    await assert.rejects(normal.listPaymentLinks({ids:[values[0].id,values[1].id]}));assert.deepEqual(gets,[]);assert.equal(writes,0);cases++;
   }
   rows.set(key,original);
   change=x=>({...x,active:x.id!=='plink_0'});
   assert.deepEqual(Array.from(await normal.listPaymentLinks({ids:selected}),v=>v.id),[values[1].id,values[1].id]);cases++;
   for(const active of [undefined,null,'true',1]){change=x=>({...x,active});await assert.rejects(normal.listPaymentLinks({ids:[values[0].id]}),/active state/);cases++;}
   change=x=>({...x,livemode:true});await assert.rejects(normal.listPaymentLinks({ids:[values[0].id]}),/does not match/);cases++;
   change=()=>{throw Error('provider unavailable');};await assert.rejects(normal.listPaymentLinks({ids:[values[0].id]}),/provider unavailable/);cases++;
   change=x=>x;const fallback=new Service(client,{},cache,telemetry,actual.validator,{...mappers,PaymentLinkMapper:{...read,readCachedDto:undefined}});
   gets=[];assert.equal((await fallback.listPaymentLinks({ids:[values[0].id]}))[0].id,values[0].id);assert.equal(lists,1);assert.deepEqual(gets,[]);assert.equal(writes,0);cases++;
   for(const value of values)assert.equal(rows.get('payment_link:'+value.id),JSON.stringify(value));
   // Empty and omitted ID selections enumerate the same cache keys, not Stripe.
   rows.clear();for(const value of values.slice(0,2))rows.set('payment_link:'+value.id,JSON.stringify(value));gets=[];lists=0;change=x=>x;
   for(const request of [undefined,{ids:[]}]){assert.deepEqual(Array.from(await normal.listPaymentLinks(request),v=>v.id),values.slice(0,2).map(v=>v.id));assert.equal(lists,0);cases++;}
   rows.clear();gets=[];for(const request of [undefined,{ids:[]}]){assert.deepEqual(Array.from(await normal.listPaymentLinks(request)),[]);assert.deepEqual(gets,[]);cases++;}

  }
  if(['checkoutSession','billingPortal'].includes(family)){
   const Service=actual.load(path.join(root,`blueprint/implementations/billing/base/services/${family}.service.ts`))[`Base${name}Service`];
   const suffix=family==='billingPortal'?'BillingPortalSession':name;
   for(const enableDatabaseBackup of [false,true]){
    let writes=0,persisted=0;
    const {safeStringify}=actual.load(path.join(root,'framework/common/src/safeStringify.ts'));
    const {safeParse}=actual.load(path.join(root,'framework/common/src/safeParse.ts'));
    const cacheRows=new Map(),cache={getTtlMilliseconds:()=>60000,async putRecord({key,value}){writes++;cacheRows.set(key,safeStringify(value));},async readRecord(key){return cacheRows.has(key)?{value:safeParse(cacheRows.get(key))}:null;}};
    const readOnlyEm=Object.assign(new FixtureEntityManager(),{create:em.create,persist(){persisted++;return {async flush(){}};},async findOneOrFail(){throw Error('cache read must not query a database entity');},assign(){throw Error('cache read must not mutate an entity');}});
    const service=new Service(readOnlyEm,cache,{},actual.validator,mappers,{enableDatabaseBackup});
    const provider={id:'provider_cache_fixture',url:'https://fixture.test/provider',opaqueTimestamp:'2026-01-01T00:00:00.000Z'};
    const created=await service[`create${suffix}`](samples[family],provider);
    const cached=await service[`get${suffix}`]({id:created.id});
    assert.deepEqual(JSON.parse(JSON.stringify(cached)),JSON.parse(JSON.stringify(created)));assert.equal(writes,1);assert.equal(persisted,Number(enableDatabaseBackup));cases++;
    assert.ok(cached.expiresAt instanceof Date);assert.notEqual(cached.expiresAt,created.expiresAt);assert.equal(cached.expiresAt.getTime(),created.expiresAt.getTime());
    if(variant==='stripe')assert.deepEqual(JSON.parse(JSON.stringify(cached.stripeFields)),provider);
    const key=[...cacheRows.keys()][0],serialized=cacheRows.get(key),withTiming={...JSON.parse(serialized),createdAt:'2026-01-01T00:00:00.000Z',updatedAt:1767225600000};
    cacheRows.set(key,JSON.stringify(withTiming));
    const timed=await service[`get${suffix}`]({id:created.id});
    assert.ok(timed.createdAt instanceof Date);assert.ok(timed.updatedAt instanceof Date);assert.equal(timed.createdAt.getTime(),1767225600000);assert.equal(timed.updatedAt.getTime(),1767225600000);assert.equal(cacheRows.get(key),JSON.stringify(withTiming));cases++;
    for(const invalid of [undefined,null,false,{},'not-a-date','9999999999999999999999']){
     cacheRows.set(key,JSON.stringify({...withTiming,expiresAt:invalid}));
     await assert.rejects(service[`get${suffix}`]({id:created.id}));cases++;
    }
    for(const field of ['createdAt','updatedAt']){
     cacheRows.set(key,JSON.stringify({...withTiming,[field]:'not-a-date'}));
     await assert.rejects(service[`get${suffix}`]({id:created.id}));cases++;
    }
    cacheRows.set(key,serialized);
    await assert.rejects(service[`get${suffix}`]({id:randomUUID()}));cases++;
    if(family==='checkoutSession'&&variant==='stripe'){
     const StripeService=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/checkoutSession.service.ts')).StripeCheckoutSessionService;
     const fresh={id:'cs_cache_owned',object:'checkout.session',mode:'subscription',livemode:false,customer:'cus_owned',url:'https://fixture.test/stripe'};
     let response=fresh;const calls=[];
     const stripe={checkout:{sessions:{async create(){return structuredClone(fresh);},async retrieve(id){calls.push(id);return structuredClone(response);}}}};
     const wrapper=new StripeService(stripe,readOnlyEm,cache,{},actual.validator,mappers,{enableDatabaseBackup});
     const made=await wrapper.createCheckoutSession(samples[family]);
     assert.match(made.id,/^[a-f0-9-]{36}$/);assert.notEqual(made.id,fresh.id);
     const ownKey='checkout_session:'+made.id,ownJson=cacheRows.get(ownKey),writesBefore=writes;
     const result=await wrapper.getCheckoutSession({id:made.id});
     assert.deepEqual(calls,['cs_cache_owned']);assert.equal(result.id,made.id);assert.equal(result.customerId,made.customerId);assert.deepEqual(result.stripeFields,fresh);assert.equal(writes,writesBefore);assert.equal(cacheRows.get(ownKey),ownJson);cases++;
     await assert.rejects(wrapper.getCheckoutSession({id:randomUUID()}));assert.equal(calls.length,1);cases++;
     for(const patch of [{id:randomUUID()},{id:'cs_bad'},{customerId:null},{expiresAt:'not-a-date'},{stripeFields:null},{stripeFields:{...fresh,id:'cs_bad/path'}},{stripeFields:{...fresh,object:'customer'}},{stripeFields:{...fresh,mode:'payment'}},{stripeFields:{...fresh,livemode:null}},{stripeFields:{...fresh,customer:undefined}},{stripeFields:{...fresh,customer:{id:'cus_owned',object:'customer',deleted:'false'}}}]){
      cacheRows.set(ownKey,JSON.stringify({...JSON.parse(ownJson),...patch}));const count=calls.length;
      await assert.rejects(wrapper.getCheckoutSession({id:made.id}));assert.equal(calls.length,count);cases++;
     }
     cacheRows.set(ownKey,ownJson);
     for(const patch of [{id:'cs_other'},{object:'payment_intent'},{mode:'payment'},{livemode:true},{customer:'cus_other'},{customer:null},{customer:{id:'cus_owned',object:'customer',deleted:true}}]){
      response={...fresh,...patch};await assert.rejects(wrapper.getCheckoutSession({id:made.id}));assert.equal(cacheRows.get(ownKey),ownJson);cases++;
     }
     response={...fresh,customer:{id:'cus_owned',object:'customer',deleted:false}};
     assert.equal((await wrapper.getCheckoutSession({id:made.id})).customerId,made.customerId);cases++;
     cacheRows.set(ownKey,JSON.stringify({...JSON.parse(ownJson),stripeFields:{...fresh,customer:null}}));response=fresh;
     assert.equal((await wrapper.getCheckoutSession({id:made.id})).customerId,made.customerId);cases++;
     for(const blank of ['', ' ', '\uFEFF', '\u2028']) {
      const blankDto={...made,id:blank,stripeFields:fresh};
      assert.equal((await mappers.CheckoutSessionMapper.readCachedDto(blankDto,async()=>fresh)).id,blank);cases++;
      cacheRows.set('checkout_session:'+blank,JSON.stringify(blankDto));const count=calls.length;
      await assert.rejects(wrapper.getCheckoutSession({id:blank}));assert.equal(calls.length,count);assert.equal(writes,writesBefore);cases++;
      cacheRows.delete('checkout_session:'+blank);
     }
     cacheRows.set(ownKey,ownJson);
     const legacyMappers={...mappers,CheckoutSessionMapper:{...mappers.CheckoutSessionMapper,readCachedDto:undefined}};
     const legacy=new StripeService(stripe,readOnlyEm,cache,{},actual.validator,legacyMappers,{enableDatabaseBackup});
     await legacy.getCheckoutSession({id:made.id});assert.equal(calls.at(-1),made.id);cases++;
     assert.equal(cacheRows.get(ownKey),ownJson);assert.equal(writes,writesBefore);
    }
    if(family==='billingPortal'&&!enableDatabaseBackup){
     records.clear(); // No persisted entity exists; readOnlyEm rejects ORM access.
     const nextProvider={id:'provider_updated_cache',opaqueTimestamp:'2026-01-01T00:00:00Z'};
     const nextExpiry=new Date('2026-12-02T00:00:00Z');
     const updated=await service.updateBillingPortalSession({id:created.id,uri:'https://fixture.test/cache-update',expiresAt:nextExpiry,customerId:'must-not-reassign',...(variant==='base'?{providerFields:nextProvider}:{stripeFields:{id:'caller-must-not-replace-provider-response'}})},nextProvider);
     assert.equal(updated.id,created.id);assert.equal(updated.customerId,created.customerId);assert.equal(updated.uri,'https://fixture.test/cache-update');assert.equal(updated.expiresAt.getTime(),nextExpiry.getTime());
     assert.deepEqual(variant==='stripe'?updated.stripeFields:updated.providerFields,nextProvider);assert.deepEqual(updated.providerFields,nextProvider);assert.equal(writes,2);assert.equal(persisted,0);
     assert.deepEqual(JSON.parse(JSON.stringify(await service.getBillingPortalSession({id:created.id}))),JSON.parse(JSON.stringify(updated)));cases++;
     const overrideManager=new FixtureEntityManager(),extraContext={request:'context'},forwarded={...mappers,UpdateBillingPortalMapper:{...update,async toCachedDto(input,existing,manager,...args){assert.equal(manager,overrideManager);assert.deepEqual(args,[nextProvider,extraContext]);return update.toCachedDto(input,existing,manager,...args);}}};
     const contextual=new Service(readOnlyEm,cache,{},actual.validator,forwarded);
     await contextual.updateBillingPortalSession({id:created.id},overrideManager,nextProvider,extraContext);cases++;
     assert.equal(writes,3);
     if(variant==='stripe'){
      let providerCalls=0;
      const StripeService=actual.load(path.join(root,'blueprint/implementations/billing/stripe/services/billingPortal.service.ts')).StripeBillingPortalService;
      const providerSession={id:'bps_replaced',object:'billing_portal.session',customer:created.customerId,url:'https://fixture.test/replaced'};
      const stripeService=new StripeService({billingPortal:{sessions:{async create(input){providerCalls++;assert.equal(input.customer,created.customerId);return providerSession;}}}},readOnlyEm,cache,{},actual.validator,mappers);
      await assert.rejects(stripeService.updateBillingPortalSession({id:created.id,customerId:'foreign-customer'}),/identity cannot change/);assert.equal(providerCalls,0);cases++;
      const validCache=cacheRows.get(key),preflightWrites=writes;
      for(const changed of [{id:randomUUID()},{id:null},{customerId:null},{customerId:7},{customerId:''},{customerId:'   '},{expiresAt:'invalid'}]){
       const malformed=JSON.stringify({...JSON.parse(validCache),...changed});cacheRows.set(key,malformed);
       await assert.rejects(stripeService.updateBillingPortalSession({id:created.id}));
       assert.equal(providerCalls,0);assert.equal(writes,preflightWrites);assert.equal(cacheRows.get(key),malformed);cases++;
      }
      cacheRows.set(key,validCache);
      for(const patch of [{expiresAt:'invalid'},{expiresAt:{}},{id:undefined}]){
       await assert.rejects(stripeService.updateBillingPortalSession({id:created.id,...patch}));
       assert.equal(providerCalls,0);assert.equal(writes,preflightWrites);assert.equal(cacheRows.get(key),validCache);cases++;
      }
      const replaced=await stripeService.updateBillingPortalSession({id:created.id,stripeFields:{return_url:'https://fixture.test/return'}});
      assert.equal(providerCalls,1);assert.equal(replaced.id,created.id);assert.equal(replaced.customerId,created.customerId);assert.equal(replaced.uri,providerSession.url);assert.deepEqual(replaced.stripeFields,providerSession);assert.equal(persisted,0);cases++;
      assert.equal(writes,4);
     }
     const before=cacheRows.get(key),beforeWrites=writes;
     await assert.rejects(service.updateBillingPortalSession({id:created.id,expiresAt:'invalid'},nextProvider));
     assert.equal(cacheRows.get(key),before);assert.equal(writes,beforeWrites);cases++;
     for(const changed of [{id:randomUUID()},{customerId:'different-customer'}]){
      const invalidMappers={...mappers,UpdateBillingPortalMapper:{...update,toCachedDto:async()=>({...updated,...changed})}};
      const invalidService=new Service(readOnlyEm,cache,{},actual.validator,invalidMappers);
      await assert.rejects(invalidService.updateBillingPortalSession({id:created.id}),/identity cannot change/);
      assert.equal(cacheRows.get(key),before);assert.equal(writes,beforeWrites);cases++;
     }
    }
   }
  }
  if(family==='billingPortal'){
   const Service=actual.load(path.join(root,'blueprint/implementations/billing/base/services/billingPortal.service.ts')).BaseBillingPortalService;
   const {safeStringify}=actual.load(path.join(root,'framework/common/src/safeStringify.ts'));
   const {safeParse}=actual.load(path.join(root,'framework/common/src/safeParse.ts'));
   // Exercise configured mappers over an existing entity. This checks one update
   // pass and returned/cache consistency; cache-only entity reconstruction stays
   // a separate extension point.
   for(const enableDatabaseBackup of [false,true]){
    let writes=0,persisted=0,updates=0,responses=0;
    const rows=new Map(),cache={getTtlMilliseconds:()=>60000,async putRecord({key,value}){writes++;rows.set(key,safeStringify(value));},async readRecord(key){return rows.has(key)?{value:safeParse(rows.get(key))}:null;}};
    const manager={...em,persist(){persisted++;return {async flush(){}};}};
    const mapped={...mappers,UpdateBillingPortalMapper:{...update,toCachedDto:undefined,async toEntity(...args){updates++;assert.equal(updates,1,'update mapper must run once');return update.toEntity(...args);}},BillingPortalMapper:{...read,async toDto(...args){responses++;return read.toDto(...args);}}};
    const service=new Service(manager,cache,{},actual.validator,mapped,{enableDatabaseBackup});
    const created=await service.createBillingPortalSession(samples.billingPortal,{id:'old_provider'});
    updates=0;responses=0;persisted=0;writes=0;
    const provider={id:'new_provider',opaqueTimestamp:'2026-01-01T00:00:00Z'};
    const changed=await service.updateBillingPortalSession({id:created.id,uri:'https://fixture.test/updated'},provider);
    assert.equal(updates,1);assert.equal(responses,1);assert.equal(writes,1);assert.equal(persisted,Number(enableDatabaseBackup));
    assert.equal(changed.uri,'https://fixture.test/updated');assert.ok(changed.expiresAt instanceof Date);
    if(variant==='stripe')assert.deepEqual(changed.stripeFields,provider);
    assert.deepEqual(JSON.parse(JSON.stringify(changed)),JSON.parse(JSON.stringify(await service.getBillingPortalSession({id:created.id}))));cases++;
    const key=[...rows.keys()][0],valid=rows.get(key);
    rows.set(key,JSON.stringify({...JSON.parse(valid),expiresAt:'invalid'}));
    updates=0;responses=0;persisted=0;writes=0;
    await assert.rejects(service.updateBillingPortalSession({id:created.id,uri:'https://fixture.test/rejected'},provider));
    assert.equal(updates,0);assert.equal(responses,0);assert.equal(persisted,0);assert.equal(writes,0);cases++;
   }
  }
  records.set(entity.id,entity);
  if(['checkoutSession','paymentLink'].includes(family)){
   // Exercise actual configured schemas and mapper calls for every retained and
   // emitted status, rather than substituting a permissive fixture enum.
   for(const status of ['PENDING','SUCCESS','CREATED','UPDATED','COMPLETED','CANCELLED','FAILED','EXPIRED']){
    const created=await create.toEntity(parse(create,{...samples[family],status}),em,{id:'provider_fixture'});
    assert.equal(parse(read,await read.toDto(created)).status,status);cases++;
    const updated=await update.toEntity(parse(update,{id:entity.id,status}),em,{id:'provider_fixture'});
    assert.equal(parse(read,await read.toDto(updated)).status,status);cases++;
   }
   for(const [mapper,input] of [[create,{...samples[family],status:'UNKNOWN'}],[update,{id:entity.id,status:'UNKNOWN'}],[read,{...dto,status:'UNKNOWN'}]]){
    assert.equal(actual.validator.parse(actual.validator.schemify(mapper.schema),input).ok,false);cases++;
   }
  }
  const patch=parse(update,{id:entity.id,undeclaredField:'strip',...(family==='billingPortal'?{customerId:'must-not-reassign',uri:'https://fixture.test/updated'}:{})});
  assert.equal(patch.undeclaredField,undefined);
  if(family==='billingPortal')assert.equal(patch.customerId,undefined);
  const changed=await update.toEntity(patch,em,variant==='stripe'?{id:'provider_updated'}:undefined);
  assert.equal(changed.id,entity.id);
  if(family==='billingPortal')assert.equal(changed.customerId,'customer');cases++;
  await assert.rejects(update.toEntity({...patch,id:randomUUID()},em,{}));cases++;
  const missing={...samples[family]};delete missing[Object.keys(missing)[0]];
  await assert.rejects(create.toEntity(missing,em,{}));cases++;
  if(variant==='stripe'){
   const providerIdInput=parse(create,{...samples[family],id:'provider_supplied_id'});
   const local=await create.toEntity(providerIdInput,em,{id:'provider_supplied_id'});
   assert.notEqual(local.id,'provider_supplied_id');cases++;
  }
 }
}
console.log(`PASS ${cases} actual configured billing mapper cases: cache DTO reads/updates without ORM access, Stripe replacement and identity guards, emitted/retained status acceptance, unknown-status rejection, schema resolution, HTTP-boundary unknown fields, optional provider payloads, local UUIDs, response mapping, ownership and missing-row failures`);
console.log('AUDIT Stripe configured mappers allocate local UUIDs and retain provider responses; configured checkout/payment-link reads resolve cached provider references, and payment-link update/expire/success/failure and checkout expire/success/failure call Stripe with the stored Stripe id. Other reference-based mutation paths still require identity integration; native scoped durable aliases are separate. Native provider payloads require objects and native scalar coercions remain stricter.');
