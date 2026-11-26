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

 const mappers=actual.load(path.join(root,`blueprint/billing-${variant}/domain/mappers/paymentLink.mappers.ts`));
 const Service=actual.load(path.join(root,`blueprint/implementations/billing/${variant}/services/paymentLink.service.ts`))[variant==='base'?'BasePaymentLinkService':'StripePaymentLinkService'];
 const id=randomUUID(),provider={id:'plink_fixture',object:'payment_link',livemode:false,active:true,nested:{date:'2026-01-01'}};
 const good={id,amount:12,currency:'USD',paymentMethods:[variant==='base'?'credit_card':'card'],status:'PENDING',createdAt:'2026-01-01T00:00:00.000Z',updatedAt:'2026-01-02T00:00:00.000Z',extension:'retained',...(variant==='base'?{providerFields:provider}:{stripeFields:provider})};
 let value=structuredClone(good),writes=0,gets=0;
 const cache={getTtlMilliseconds:()=>1000,async readRecord(){return {value:structuredClone(value)};},async listKeys(){return ['paymentLink:'+id];},async putRecord(){writes++;}};
 const client={paymentLinks:{async retrieve(){gets++;return structuredClone(provider);}}};
 const em=new Proxy({}, {get(){throw Error('no ORM allowed');}});
 const service=variant==='base'?new Service(em,cache,{},actual.validator,mappers):new Service(client,em,cache,{log(){}},actual.validator,mappers);
 for(const method of ['getPaymentLink','listPaymentLinks']){
  value=structuredClone(good);const result=await service[method](method==='getPaymentLink'?{id}:{ids:[id]});const dto=Array.isArray(result)?result[0]:result;
  assert.ok(dto.createdAt instanceof Date);assert.ok(dto.updatedAt instanceof Date);assert.equal(dto.extension,'retained');assert.equal((dto.providerFields??dto.stripeFields).nested.date,'2026-01-01');assert.deepEqual(value,good);assert.equal(writes,0);cases++;
  for(const changed of [{id:randomUUID()},{createdAt:'invalid'},{updatedAt:{}},{amount:'invalid'}]){
   value={...good,...changed};const before=gets;await assert.rejects(service[method](method==='getPaymentLink'?{id}:{ids:[id]}));assert.equal(gets,before);assert.equal(writes,0);cases++;
  }
 }
 const legacyMappers={...mappers,PaymentLinkMapper:{...mappers.PaymentLinkMapper,normalizeCachedDto:undefined,readCachedDto:undefined,validateCachedRead:undefined}};
 value=structuredClone(good);const Base=actual.load(path.join(root,'blueprint/implementations/billing/base/services/paymentLink.service.ts')).BasePaymentLinkService;
 assert.equal(typeof (await new Base(em,cache,{},actual.validator,legacyMappers).getPaymentLink({id})).createdAt,'string');cases++;
 const badHook=new Base(em,cache,{},actual.validator,{...mappers,PaymentLinkMapper:{...mappers.PaymentLinkMapper,normalizeCachedDto:value=>({...value,id:randomUUID()})}});await assert.rejects(badHook.getPaymentLink({id}));assert.equal(writes,0);cases++;
 if(variant==='stripe'){
  const a=randomUUID(),b=randomUUID(),c=randomUUID();
  const rows=new Map([[a,{...good,id:a,stripeFields:{...provider,id:'plink_a'}}],[b,{...good,id:b,stripeFields:{...provider,id:'plink_b'}}],[c,{...good,id:c,stripeFields:{...provider,id:'plink_c'}}]]);
  const providers=new Map([...rows.values()].map(v=>[v.stripeFields.id,{...v.stripeFields,active:v.id!==b}]));
  let reads=0; const fetched=[];
  const listCache={...cache,async readRecord(key){reads++;const row=rows.get(key.slice('payment_link:'.length));return row?{value:structuredClone(row)}:undefined;}};
  const listClient={paymentLinks:{async retrieve(id){fetched.push(id);return structuredClone(providers.get(id));},async list(){throw Error('account scan forbidden');}}};
  const make=options=>new Service(listClient,em,listCache,{log(){}},actual.validator,mappers,options);
  const listService=make();
  const got=await listService.listPaymentLinks({ids:[c,b,a,c]});assert.deepEqual(Array.from(got,v=>v.id),[c,a,c]);assert.deepEqual(fetched,['plink_c','plink_b','plink_a','plink_c']);assert.ok(got.every(v=>v.amount===good.amount&&v.status===good.status));cases++;
  for(const ids of [[a,randomUUID()],Array(101).fill(a),[a,' ']]){const n=fetched.length;await assert.rejects(listService.listPaymentLinks({ids}));assert.equal(fetched.length,n);cases++;}
  const selected=await make({maxConfiguredListRecords:150}).listPaymentLinks({ids:Array(150).fill(a)});assert.equal(selected.length,150);cases++;
  for(const active of [undefined,null,'true',1,{}]){providers.get('plink_a').active=active;await assert.rejects(listService.listPaymentLinks({ids:[a]}));cases++;}
  for(const maxConfiguredListRecords of [0,1001,1.5]){assert.throws(()=>make({maxConfiguredListRecords}));cases++;}
  assert.ok(reads>0);assert.equal(writes,0);
 }

}
console.log(`PASS ${cases} configured PaymentLink JSON cache DTO reads/lists: Date restoration, exact payloads, zero ORM/writes, pre-provider denial and custom fallback`);
