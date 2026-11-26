// Actual configured mapper/service and real MikroORM fresh-field query checks.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {randomUUID} from 'node:crypto';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const ormDependencies=process.env.FORKLAUNCH_TEST_ORM_DEPENDENCIES;
assert.ok(ormDependencies,'Set FORKLAUNCH_TEST_ORM_DEPENDENCIES to the installed fixture dependency root');
const ormLib=require(path.join(ormDependencies,'node_modules/@mikro-orm/postgresql'));
const Payment=new ormLib.EntitySchema({name:'ResumePayment',tableName:'resume_payment',properties:{id:{type:'string',primary:true},orderId:{type:'string'},providerRef:{type:'string',nullable:true},amountCents:{type:'integer'},currency:{type:'string'},status:{type:'string'}}});
const Order=new ormLib.EntitySchema({name:'ResumeOrder',tableName:'resume_order',properties:{id:{type:'string',primary:true},status:{type:'string'},shippingAddress:{type:'json',nullable:true}}});
let actual;
actual=actualValidator(root,(spec,file,load)=>{
 if(spec==='@forklaunch/validator/zod')return actual.statics;
 if(spec==='../../schema')return {schemaValidator:actual.validator};
 if(spec==='@forklaunch/core/http')return {};
 if(spec==='@mikro-orm/core')return ormLib;
 if(spec.endsWith('/persistence/entities/payment.entity'))return {Payment};
 if(spec.endsWith('/persistence/entities/order.entity'))return {Order};
 if(spec==='@forklaunch/interfaces-ecommerce/types')return {...load(path.join(root,'blueprint/interfaces/ecommerce/types/order.service.types.ts')),...load(path.join(root,'blueprint/interfaces/ecommerce/types/payment.service.types.ts'))};
 if(spec==='@forklaunch/core/mappers')return load(path.join(root,'framework/core/src/mappers/mapper.ts'));
 if(spec==='../schemas')return {PaymentSchemas:load(path.join(root,'blueprint/implementations/ecommerce/base/domain/schemas/zod/payment.schema.ts'))};
 if(spec==='@forklaunch/implementation-ecommerce-base/services')return load(path.join(root,'blueprint/implementations/ecommerce/base/services/payment.service.ts'));
},{sameRealm:true});
const mappers=actual.load(path.join(root,'blueprint/ecommerce-stripe/domain/mappers/payment.mappers.ts'));
const Service=actual.load(path.join(root,'blueprint/implementations/ecommerce/stripe/services/payment.service.ts')).StripePaymentService;
const original={id:'payment',orderId:'order',providerRef:'pi_owned',amountCents:100,currency:'usd',status:'pending'};
let cases=0;
for(const status of ['pending','paid','fulfilled','shipped','delivered','cancelled','invalid',null]){
 let payment=structuredClone(original),order=status===null?null:{id:'order',status},calls=0,queries=[];
 const em={isInTransaction:()=>false,async findOneOrFail(entity,where,options){queries.push({entity,where,options});const row=entity===Payment?payment:order;if(!row)throw Error('missing');return structuredClone(row);}};
 const service=new Service({paymentIntents:{async retrieve(id){calls++;assert.equal(id,'pi_owned');return {id,client_secret:'owned-secret'};}}},em,{},actual.validator,mappers);
 if(status==='cancelled'||status==='invalid'||status===null){await assert.rejects(service.resumePayment({id:'payment'}));assert.equal(calls,0);}else{assert.equal((await service.resumePayment({id:'payment'})).clientSecret,'owned-secret');assert.equal(calls,1);}
 for(const query of queries.filter(q=>q.options)){assert.equal(query.options.refresh,true);assert.deepEqual(query.options.fields,query.entity===Payment?['id','orderId','providerRef']:['id','status']);}
 assert.deepEqual(payment,original);cases++;
}
for(const change of ['cancel','missing','order-id','payment-id','reference','relation']){
 let payment=structuredClone(original),order={id:'order',status:'pending'},calls=0;
 const em={isInTransaction:()=>false,async findOneOrFail(entity){const row=entity===Payment?payment:order;if(!row)throw Error('missing');return structuredClone(row);}};
 const service=new Service({paymentIntents:{async retrieve(){calls++;if(change==='cancel')order.status='cancelled';if(change==='missing')order=null;if(change==='order-id')order.id='other';if(change==='payment-id')payment.id='other';if(change==='reference')payment.providerRef='pi_other';if(change==='relation')payment.orderId='other';return {id:'pi_owned',client_secret:'must-not-return'};}}},em,{},actual.validator,mappers);
 await assert.rejects(service.resumePayment({id:'payment'}));assert.equal(calls,1);cases++;
}
// Provider response identity is required even for custom mappers without a local guard.
for(const guard of [true,false]){
 for(const response of [{id:'pi_other',client_secret:'must-not-return'},{client_secret:'must-not-return'},{id:42,client_secret:'must-not-return'},{id:null,client_secret:'must-not-return'},null]){
  let calls=0;const payment=structuredClone(original);
  const em={isInTransaction:()=>false,async findOneOrFail(entity){return entity===Payment?structuredClone(payment):{id:'order',status:'pending'};}};
  const service=new Service({paymentIntents:{async retrieve(id){calls++;assert.equal(id,'pi_owned');return response;}}},em,{},actual.validator,{...mappers,PaymentMapper:{...mappers.PaymentMapper,validateResume:guard?mappers.PaymentMapper.validateResume:undefined}});
  await assert.rejects(service.resumePayment({id:'payment'}),error=>error.message==='Stripe returned an invalid PaymentIntent response');
  assert.equal(calls,1);assert.deepEqual(payment,original);cases++;
 }
 // The custom-mapper fallback still accepts an exactly matching provider response.
 let calls=0;const em={isInTransaction:()=>false,async findOneOrFail(entity){return entity===Payment?structuredClone(original):{id:'order',status:'pending'};}};
 const service=new Service({paymentIntents:{async retrieve(id){calls++;return {id,client_secret:'owned-secret'};}}},em,{},actual.validator,{...mappers,PaymentMapper:{...mappers.PaymentMapper,validateResume:guard?mappers.PaymentMapper.validateResume:undefined}});
 assert.equal((await service.resumePayment({id:'payment'})).clientSecret,'owned-secret');assert.equal(calls,1);cases++;
}
// Generic custom mapper compatibility and provider-less configured checks.
for(const guard of [true,false]){let calls=0;const payment={...original,providerRef:undefined};const em={isInTransaction:()=>false,async findOneOrFail(entity){if(entity===Order)throw Error('missing order');return {...payment};}};const service=new Service({paymentIntents:{async retrieve(){calls++;throw Error('unexpected');}}},em,{},actual.validator,{...mappers,PaymentMapper:{...mappers.PaymentMapper,validateResume:guard?mappers.PaymentMapper.validateResume:undefined}});if(guard)await assert.rejects(service.resumePayment({id:'payment'}));else assert.equal((await service.resumePayment({id:'payment'})).clientSecret,undefined);assert.equal(calls,0);cases++;}
if(process.env.FORKLAUNCH_TEST_POSTGRES_URL){
 const schema='resume_guard_'+randomUUID().replaceAll('-','');const queries=[];
 const orm=await ormLib.MikroORM.init({entities:[Payment,Order],clientUrl:process.env.FORKLAUNCH_TEST_POSTGRES_URL,schema,debug:['query'],logger:line=>queries.push(line)});
 try{
  await orm.schema.create();const em=orm.em.fork();await em.insert(Order,{id:'order',status:'pending',shippingAddress:{private:'not-selected'}});await em.insert(Payment,original);
  // Populate an identity-map value, then change the durable row elsewhere.
  await em.findOneOrFail(Order,{id:'order'});await em.findOneOrFail(Payment,{id:'payment'});
  await orm.em.fork().nativeUpdate(Order,{id:'order'},{status:'cancelled'});queries.length=0;
  await assert.rejects(mappers.PaymentMapper.validateResume(original,em));
  assert.ok(queries.some(q=>q.includes('select')&&q.includes('resume_order')));assert.ok(queries.every(q=>!q.includes('shipping_address')));cases++;
  await orm.em.fork().nativeUpdate(Order,{id:'order'},{status:'paid'});await mappers.PaymentMapper.validateResume(original,em);cases++;
  let providerCalls=0;
  const guarded=new Service({paymentIntents:{async retrieve(){providerCalls++;throw Error('unexpected provider call');}}},em,{},actual.validator,mappers);
  await em.transactional(async inner=>{await assert.rejects(guarded.resumePayment({id:'payment'},inner),/outside a transaction/);});assert.equal(providerCalls,0);cases++;
  await orm.em.fork().nativeUpdate(Payment,{id:'payment'},{providerRef:'pi_replaced'});await assert.rejects(mappers.PaymentMapper.validateResume(original,em));cases++;
 }finally{await orm.schema.drop({dropDb:false,dropMigrationsTable:true});await orm.close(true);}
}
console.log(`PASS ${cases} actual configured ecommerce resume guard cases; no provider mutation, exact relation checks, fresh selected ORM fields and legacy fallback`);
