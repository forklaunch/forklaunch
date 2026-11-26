import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const compiler=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(compiler))]}));
const cacheDtoExports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'blueprint/implementations/billing/base/utils/cachedSessionDto.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:cacheDtoExports,Date});
const clone=value=>value===undefined?undefined:structuredClone(value);
function matches(row,where={}){return Object.entries(where).every(([key,value])=>value&&typeof value==='object'&&'$in'in value?value.$in.includes(row[key]):row[key]===value);}
// Only ORM, cache and generic mapper boundaries are owned. Business rules execute
// the repository's actual base classes; configured Stripe mappers are a separate gate.
class EntityManager {
 constructor(){this.rows=new Map();}
 table(entity){if(!this.rows.has(entity))this.rows.set(entity,new Map());return this.rows.get(entity);}
 async transactional(callback){return callback(this);}
 persist(row){this.table(row.entity).set(row.id,row);return{flush:async()=>{}};}
 async upsert(entityOrRow,data){if(data&&!this.table(entityOrRow).has(data.id))throw Error('Cannot insert an incomplete required billing row');const row=data?{...(this.table(entityOrRow).get(data.id)??{}),entity:entityOrRow,...clone(data)}:entityOrRow;this.persist(row);return row;}
 async findAll(entity,{where}={}){return[...this.table(entity).values()].filter(row=>matches(row,where));}
 async findOne(entity,where){return(await this.findAll(entity,{where}))[0]??null;}
 async findOneOrFail(entity,where){const row=await this.findOne(entity,where);if(!row)throw Error('not found');return row;}
 remove(row){this.table(row.entity).delete(row.id);return{flush:async()=>{}};}
 async nativeDelete(entity,where){for(const row of await this.findAll(entity,{where}))this.table(entity).delete(row.id);}
}
class Cache {
 constructor(){this.rows=new Map();}
 getTtlMilliseconds(){return 600000;}
 async putRecord({key,value}){this.rows.set(key,clone(value));}
 async readRecord(key){return this.rows.has(key)?{key,value:clone(this.rows.get(key))}:null;}
 async deleteRecord(key){this.rows.delete(key);}
 async listKeys(prefix){return[...this.rows.keys()].filter(key=>key.startsWith(prefix));}
}
const names=['Plan','Subscription','CheckoutSession','PaymentLink','BillingPortal'],entities=Object.fromEntries(names.map(name=>[name,name]));
const classes={};for(const name of names){const exports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'blueprint/implementations/billing/base/services',name[0].toLowerCase()+name.slice(1)+'.service.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Date,require(spec){if(spec==='../utils/cachedSessionDto')return cacheDtoExports;if(spec==='@mikro-orm/core')return{EntityManager};if(spec==='@forklaunch/core/http')return{evaluateTelemetryOptions:()=>({enabled:{}})};if(spec==='@forklaunch/core/cache')return{createCacheKey:prefix=>id=>prefix+':'+id};if(spec.endsWith('/persistence/entities'))return entities;throw Error('Unexpected runtime import '+spec);}});classes[name]=exports['Base'+name+'Service'];}
const actions=[],add=(service,method,input={})=>actions.push({service,method,input});
const plan={id:'plan1',name:'Pro',price:1999,cadence:'month',currency:'USD',externalId:'price1',billingProvider:'stripe',active:true,features:['api'],providerFields:{nested:{value:1}}};
add('Plan','createPlan',plan);add('Plan','createPlan',{...plan,id:'plan2',externalId:'price2',name:'Team'});
for(const input of [{},{ids:[]},{ids:['plan2']},{ids:['missing']}])add('Plan','listPlans',input);
add('Plan','getPlan',{id:'plan1'});add('Plan','updatePlan',{id:'plan1',price:0,active:false,description:'Updated',features:[]});add('Plan','getPlan',{id:'plan1'});add('Plan','getPlan',{id:'missing'});add('Plan','deletePlan',{id:'missing'});
const subscription={id:'sub1',partyId:'user1',partyType:'USER',productId:'plan1',active:true,externalId:'subscription1',startDate:'2026-01-01T00:00:00.000Z',status:'ACTIVE',billingProvider:'stripe'};
add('Subscription','createSubscription',subscription);add('Subscription','createSubscription',{...subscription,id:'sub2',partyId:'org1',partyType:'ORGANIZATION',externalId:'subscription2',active:false});
add('Subscription','getUserSubscription',{id:'user1'});add('Subscription','getOrganizationSubscription',{id:'org1'});add('Subscription','resumeSubscription',{id:'sub2'});add('Subscription','getOrganizationSubscription',{id:'org1'});add('Subscription','cancelSubscription',{id:'sub1'});add('Subscription','getUserSubscription',{id:'user1'});add('Subscription','getSubscription',{id:'sub1'});add('Subscription','resumeSubscription',{id:'sub1'});add('Subscription','updateSubscription',{id:'sub1',status:'TRIALING',endDate:'2027-01-01T00:00:00.000Z'});
for(const externalId of ['subscription1','missing'])add('Subscription','findSubscriptionIdByExternalId',{externalId});
for(const input of [{},{ids:[]},{ids:['sub2']},{ids:['missing']}])add('Subscription','listSubscriptions',input);
for(const method of ['deleteSubscription','cancelSubscription','resumeSubscription'])add('Subscription',method,{id:'missing'});
const checkout={id:'checkout1',customerId:'customer',paymentMethods:['card'],currency:'USD',uri:'https://checkout.example/one',expiresAt:'2020-01-01T00:00:00.000Z',status:'PENDING',providerFields:{customer:'private-provider-value'}};
add('CheckoutSession','createCheckoutSession',checkout);add('CheckoutSession','getCheckoutSession',{id:checkout.id});add('CheckoutSession','expireCheckoutSession',{id:checkout.id});add('CheckoutSession','getCheckoutSession',{id:checkout.id});add('Inspect','CheckoutSession',{id:checkout.id});
for(const method of ['handleCheckoutSuccess','handleCheckoutFailure']){const id=method;add('CheckoutSession','createCheckoutSession',{...checkout,id});add('CheckoutSession',method,{id});add('CheckoutSession','getCheckoutSession',{id});add('Inspect','CheckoutSession',{id});}
add('CheckoutSession','expireCheckoutSession',{id:'missing'});add('CheckoutSession','handleCheckoutSuccess',{id:'missing'});
const link={id:'link1',amount:299,paymentMethods:['card'],currency:'USD',status:'PENDING'};
add('PaymentLink','createPaymentLink',link);add('PaymentLink','getPaymentLink',{id:link.id});add('PaymentLink','updatePaymentLink',{id:link.id,amount:0,status:'FAILED'});add('PaymentLink','getPaymentLink',{id:link.id});add('PaymentLink','createPaymentLink',{...link,id:'link2'});
for(const input of [{},{ids:[]},{ids:['link2','link1','link2']},{ids:['missing']}])add('PaymentLink','listPaymentLinks',input);
for(const method of ['expirePaymentLink','handlePaymentSuccess','handlePaymentFailure']){const id=method;add('PaymentLink','createPaymentLink',{...link,id});add('PaymentLink',method,{id});add('PaymentLink','getPaymentLink',{id});add('Inspect','PaymentLink',{id});add('PaymentLink',method,{id:'missing'});}
add('PaymentLink','updatePaymentLink',{id:'missing',amount:100});
const portal={id:'portal1',customerId:'customer',expiresAt:'2020-01-01T00:00:00.000Z',uri:'https://portal.example/session',providerFields:null};
add('BillingPortal','createBillingPortalSession',portal);add('BillingPortal','getBillingPortalSession',{id:portal.id});add('BillingPortal','updateBillingPortalSession',{id:portal.id,uri:'https://portal.example/updated'});add('BillingPortal','getBillingPortalSession',{id:portal.id});add('BillingPortal','expireBillingPortalSession',{id:portal.id});add('BillingPortal','getBillingPortalSession',{id:portal.id});add('Inspect','BillingPortal',{id:portal.id});add('BillingPortal','expireBillingPortalSession',{id:'missing'});add('BillingPortal','updateBillingPortalSession',{id:'missing',uri:'https://portal.example/missing'});
add('Plan','deletePlan',{id:'plan1'});add('Plan','getPlan',{id:'plan1'});add('Subscription','deleteSubscription',{id:'sub1'});add('Subscription','getSubscription',{id:'sub1'});
function normalize(value){return JSON.parse(JSON.stringify(value,(key,item)=>['createdAt','updatedAt','entity'].includes(key)?undefined:['startDate','endDate','expiresAt'].includes(key)?new Date(item).toISOString():item));}
let count=0;
for(const backup of [false,true]){
 const em=new EntityManager(),cache=new Cache(),services={};
 for(const name of names){
  const mapper={entity:name,async toDto(row){const{entity,...dto}=row;return clone(dto);}};
  // Generic mapper identity preserves full DTOs even when database backup is disabled.
  const identity=new Map();
  const mappers={[name+'Mapper']:mapper,['Create'+name+'Mapper']:{async toEntity(input){const row={entity:name,...clone(input)};identity.set(row.id,row);return row;}},['Update'+name+'Mapper']:{async toEntity(input){const row={...(em.table(name).get(input.id)??identity.get(input.id)??{}),entity:name,...clone(input)};identity.set(row.id,row);return row;}}};
  const telemetry={info(){},warn(){},error(){}};
  services[name]=name==='Plan'?new classes[name](em,telemetry,{},mappers):name==='Subscription'?new classes[name](em,telemetry,{},mappers):new classes[name](em,cache,telemetry,{},mappers,{enableDatabaseBackup:backup});
 }
 const expected=[];
 for(const {service,method,input}of actions){try{let result;if(service==='Inspect')result=em.table(method).get(input.id)??null;else result=await services[service][method](clone(input));expected.push({ok:normalize(result??null)});}catch{expected.push({error:true});}}
 const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-base','--example','billing_parity'],{cwd:root,input:JSON.stringify({backup,actions}),encoding:'utf8',timeout:120000}));
 assert.equal(actual.length,expected.length);
 for(let index=0;index<actual.length;index++){assert.deepEqual(normalize(actual[index]),expected[index],JSON.stringify({backup,index,action:actions[index]}));count++;}
}
console.log(`PASS ${count} actual TypeScript/Rust billing base operations: five services, party lookups, lifecycle, cache primary/backup semantics and missing-record guards`);
