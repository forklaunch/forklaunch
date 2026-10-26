// Actual configured mapper/service over owned transaction fixtures, plus SDK GET wire checks.
import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import http from 'node:http';
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
const original={id:'payment',orderId:'order',providerRef:'pi_owned',amountCents:105,currency:'usd',status:'pending'};
for(const interrupted of [true,false]){
 let durable=structuredClone(original), writes=0,providerCalls=0;
 const em={async findOneOrFail(_entity,where){assert.deepEqual(where,{providerRef:'pi_owned'});return structuredClone(durable);},assign:Object.assign,async transactional(callback){let candidate;await callback({async persist(value){writes++;candidate=structuredClone(value);}});if(interrupted)throw Error('owned transaction acknowledgement failure');durable=candidate;}};
 const service=new Service(new Proxy({}, {get(){providerCalls++;throw Error('unexpected provider access');}}),em,{},actual.validator,mappers);
 if(interrupted){await assert.rejects(service.confirmPayment({providerRef:'pi_owned'}),/owned transaction/);assert.equal(durable.status,'pending');}
 else{assert.equal((await service.confirmPayment({providerRef:'pi_owned'})).status,'succeeded');assert.equal(durable.status,'succeeded');}
 assert.equal(providerCalls,0);assert.equal(writes,1);assert.equal(durable.providerRef,'pi_owned');
}
// Official SDK wire proof only; native HTTP and evidence acceptance are tested in PG/unit fixtures.
assert(process.env.STRIPE_SDK_PATH,'Set STRIPE_SDK_PATH');
const Stripe=require(process.env.STRIPE_SDK_PATH);let calls=[];let connected=false;
const server=http.createServer((req,res)=>{
 assert.equal(req.method,'GET');assert.equal(req.headers.authorization,'Bearer capture-owned');assert.equal(req.headers['stripe-account'],connected?'acct_owned':undefined);
 const url=new URL(req.url,'http://fixture.invalid');calls.push(url.pathname);
 let value;
 if(url.pathname==='/v1/account'){assert.equal(url.search,'');value={object:'account',id:'acct_owned'};}
 else if(url.pathname==='/v1/balance'){assert.equal(url.search,'');value={object:'balance',livemode:false};}
 else{assert.equal(url.pathname,'/v1/payment_intents/pi_owned');assert.deepEqual([...url.searchParams],[['expand[0]','latest_charge.balance_transaction']]);value={id:'pi_owned',object:'payment_intent',latest_charge:{id:'ch_owned',balance_transaction:{id:'txn_owned',status:'pending'}}};}
 res.setHeader('content-type','application/json');res.end(JSON.stringify(value));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const stripe=new Stripe('capture-owned',{host:'127.0.0.1',port:server.address().port,protocol:'http',maxNetworkRetries:0});
 for(const mode of [false,true]){
  connected=mode;calls=[];const options=connected?{stripeAccount:'acct_owned'}:undefined;
  assert.equal((await stripe.accounts.retrieveCurrent({},options)).id,'acct_owned');
  assert.equal((await stripe.balance.retrieve({},options)).livemode,false);
  const value=await stripe.paymentIntents.retrieve('pi_owned',{expand:['latest_charge.balance_transaction']},options);
  assert.equal(value.latest_charge.balance_transaction.id,'txn_owned');
  assert.deepEqual(calls,['/v1/account','/v1/balance','/v1/payment_intents/pi_owned']);
 }
}finally{await new Promise(resolve=>server.close(resolve));}
console.log(`PASS actual configured TS confirmation success/interruption (zero provider calls), official Stripe ${Stripe.PACKAGE_VERSION} direct/Connected three-GET account headers and nested capture expansion; no live capture or settlement claim`);
