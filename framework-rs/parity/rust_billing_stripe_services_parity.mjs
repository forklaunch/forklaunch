import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawn} from 'node:child_process';
import http from 'node:http';
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

const Stripe=require(process.env.STRIPE_SDK_PATH??require.resolve('stripe',{paths:[path.dirname(await realpath(compiler))]}));
const wrappers={};
for(const name of names){const exports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'blueprint/implementations/billing/stripe/services',name[0].toLowerCase()+name.slice(1)+'.service.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Date,require(spec){if(spec==='@forklaunch/implementation-billing-base/services')return Object.fromEntries(names.map(n=>['Base'+n+'Service',classes[n]]));throw Error('Unexpected wrapper runtime import '+spec);}});wrappers[name]=exports['Stripe'+name+'Service'];}
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','services_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const target=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'})).target_directory;
const actions=[],add=(service,method,input={})=>actions.push({service,method,input});
const plan={id:'plan1',name:'prod_fixture',price:1999,cadence:'month',currency:'USD',externalId:'ignored',billingProvider:'ignored',active:true,stripeFields:{metadata:{note:'private-plan'},amount:999}};
add('Plan','createPlan',plan);add('Plan','getPlan',{id:'plan1'});add('Plan','updatePlan',{id:'plan1',price:0,active:false,stripeFields:{metadata:{note:'replacement'}}});add('Plan','getPlan',{id:'plan1'});add('Plan','listPlans');
const sub={id:'sub1',partyId:'cus_fixture',partyType:'USER',productId:'plan1',active:true,externalId:'ignored',billingProvider:'ignored',startDate:'2026-01-01T00:00:00.000Z',status:'ACTIVE',stripeFields:{trial_period_days:7,customer:'must-be-overridden'}};
add('Subscription','createSubscription',sub);add('Subscription','getSubscription',{id:'sub1'});add('Subscription','getUserSubscription',{id:'cus_fixture'});add('Subscription','updateSubscription',{id:'sub1',productId:'plan_other',stripeFields:{metadata:{note:'changed'}}});add('Subscription','cancelSubscription',{id:'sub1'});add('Subscription','getUserSubscription',{id:'cus_fixture'});add('Subscription','getSubscription',{id:'sub1'});add('Subscription','resumeSubscription',{id:'sub1'});add('Subscription','getSubscription',{id:'sub1'});add('Subscription','listSubscriptions');
const checkout={customerId:'cus_fixture',paymentMethods:['card'],currency:'USD',uri:'ignored',successRedirectUri:'https://example.test/success',cancelRedirectUri:'https://example.test/cancel',expiresAt:'2020-01-01T00:00:00.000Z',status:'PENDING',stripeFields:{line_items:[{price:'price_fixture',quantity:1}],mode:'must-be-overridden'}};
for(const [index,method]of ['expireCheckoutSession','handleCheckoutSuccess','handleCheckoutFailure'].entries()){add('CheckoutSession','createCheckoutSession',checkout);add('CheckoutSession','getCheckoutSession',{id:'cs_'+(index+1)});add('CheckoutSession',method,{id:'cs_'+(index+1)});add('CheckoutSession','getCheckoutSession',{id:'cs_'+(index+1)});add('Inspect','CheckoutSession',{id:'cs_'+(index+1)});}
const link={amount:299,paymentMethods:['card'],currency:'USD',status:'PENDING',stripeFields:{line_items:[{price:'price_fixture',quantity:1}]}};
for(const stripeFields of [undefined,null,false,1,'bad',[],{}, {line_items:null},{line_items:{}},{line_items:'bad'},{line_items:[null]},{line_items:[1]},{line_items:[[]]}]){add('PaymentLink','createPaymentLink',{...link,stripeFields});actions.at(-1).expectNoProvider=true;}

for(const [index,method]of ['expirePaymentLink','handlePaymentSuccess','handlePaymentFailure'].entries()){const id='plink_'+(index+1);add('PaymentLink','createPaymentLink',link);add('PaymentLink','getPaymentLink',{id});add('PaymentLink','updatePaymentLink',{id,amount:888,paymentMethods:['card'],stripeFields:{metadata:{note:'updated'}}});add('PaymentLink','listPaymentLinks');add('PaymentLink',method,{id});add('PaymentLink','getPaymentLink',{id});add('Inspect','PaymentLink',{id});}
const portal={customerId:'cus_fixture',expiresAt:'2020-01-01T00:00:00.000Z',stripeFields:{return_url:'https://example.test/account'}};
add('BillingPortal','createBillingPortalSession',portal);add('BillingPortal','getBillingPortalSession',{id:'bps_1'});add('BillingPortal','updateBillingPortalSession',{id:'bps_1',customerId:'cus_other',stripeFields:{return_url:'https://example.test/other'}});add('BillingPortal','getBillingPortalSession',{id:'bps_1'});add('BillingPortal','getBillingPortalSession',{id:'bps_2'});add('BillingPortal','expireBillingPortalSession',{id:'bps_1'});add('BillingPortal','getBillingPortalSession',{id:'bps_1'});
add('Plan','deletePlan',{id:'plan1'});add('Plan','getPlan',{id:'plan1'});add('Subscription','deleteSubscription',{id:'sub1'});add('Subscription','getSubscription',{id:'sub1'});
add('Plan','createPlan',{...plan,id:'plan-failed',stripeFields:{metadata:{failure:'reject'}}});add('Plan','getPlan',{id:'plan-failed'});
add('Subscription','createSubscription',{...sub,id:'org-sub',partyId:'org_fixture',partyType:'ORGANIZATION'});add('Subscription','getOrganizationSubscription',{id:'org_fixture'});add('Subscription','updateSubscription',{id:'org-sub',stripeFields:{metadata:{failure:'reject'}}});add('Subscription','getSubscription',{id:'org-sub'});
add('BillingPortal','createBillingPortalSession',{...portal,stripeFields:{metadata:{failure:'reject'}}});
add('CheckoutSession','createCheckoutSession',{...checkout,successRedirectUri:undefined,cancelRedirectUri:undefined,stripeFields:{line_items:[{price:'price_fixture',quantity:1}],success_url:'must-be-erased',cancel_url:'must-be-erased'}});
add('PaymentLink','createPaymentLink',link);add('PaymentLink','updatePaymentLink',{id:'plink_4',stripeFields:{payment_method_types:['must-be-erased']}});
let counts={},records=new Map(),captured=[];
const server=http.createServer(async(request,response)=>{
 let raw='';for await(const chunk of request)raw+=chunk;const url=new URL(request.url,'http://fixture'),fields=new URLSearchParams(request.method==='GET'?url.search:raw);
 assert.equal(request.headers.authorization,'Bearer sk_fixture');assert.equal(request.headers['stripe-version'],Stripe.API_VERSION);
 captured.push({method:request.method,path:url.pathname,fields:[...fields].sort(([a],[b])=>a.localeCompare(b))});
 const match=url.pathname.match(/^\/v1\/(plans|subscriptions|checkout\/sessions|payment_links|billing_portal\/sessions)(?:\/([^/]+))?(?:\/(resume|expire))?$/);assert.ok(match,url.pathname);const [,resource,existing,action]=match;
 const prefix={plans:'plan_',subscriptions:'sub_', 'checkout/sessions':'cs_',payment_links:'plink_', 'billing_portal/sessions':'bps_'}[resource];
 if(fields.get('metadata[failure]')==='reject'){response.writeHead(500);response.end('{"error":{"type":"api_error","message":"sk_fixture private@example.test"}}');return;}
 const id=existing??prefix+(counts[resource]=(counts[resource]??0)+1);
 let value=records.get(id);
 if(request.method==='GET'&&!existing){response.end(JSON.stringify({object:'list',data:[...records.values()].filter(v=>v.id.startsWith(prefix)),has_more:false}));return;}
 if(!value&&existing){response.writeHead(404);response.end('{"error":{"type":"invalid_request_error","message":"private missing"}}');return;}
 if(action==='resume'&&value?.status==='canceled'){response.writeHead(400);response.end('{"error":{"type":"invalid_request_error","message":"Canceled subscriptions cannot resume"}}');return;}
 if(!existing){value={id,object:({plans:'plan',subscriptions:'subscription','checkout/sessions':'checkout.session',payment_links:'payment_link','billing_portal/sessions':'billing_portal.session'})[resource]??resource,product:{id:'prod_fixture',name:'Fixture'},interval:fields.get('interval')??'month',currency:fields.get('currency')??'USD',amount:Number(fields.get('amount')??299),url:'https://provider.example/'+id,items:{data:[{id:'si_1',plan:{id:fields.get('items[0][plan]')??'plan_fixture'}}]},line_items:{data:[{amount_total:123},{amount_total:176}]}};records.set(id,value);}
 if(request.method==='DELETE'){if(resource==='subscriptions'){value.status='canceled';}else{records.delete(id);value={id,deleted:true};}}
 if(request.method==='POST'&&existing&&!action){if(fields.has('items[0][plan]'))value.items.data[0].plan.id=fields.get('items[0][plan]');if(fields.has('metadata[status]'))value.metadata={status:fields.get('metadata[status]')};}
 response.setHeader('content-type','application/json');response.end(JSON.stringify(value));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port,base='http://127.0.0.1:'+port;
function normalize(value){return JSON.parse(JSON.stringify(value,(key,item)=>['createdAt','updatedAt','entity','providerFields'].includes(key)?undefined:key==='expiresAt'?'provider-session-expiry':['startDate','endDate'].includes(key)?new Date(item).toISOString():item));}
function nativeNames(value){if(Array.isArray(value))return value.map(nativeNames);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k==='providerFields'?'stripeFields':k,nativeNames(v)]));return value;}
let count=0;
try{for(const backup of [false,true]){
 counts={};records=new Map();captured=[];
 const em=new EntityManager(),cache=new Cache(),services={},stripe=new Stripe('sk_fixture',{host:'127.0.0.1',port,protocol:'http',maxNetworkRetries:0,timeout:1000});
 for(const name of names){const identity=new Map();
  const mapper={entity:name,async toDto(row){const{entity,providerFields,...dto}=row;return clone(dto);}};
  function mapped(input,provider){const {stripeFields,...rawDto}=clone(input),dto=Object.fromEntries(Object.entries(rawDto).filter(([,v])=>v!==undefined));return{...dto,...(provider===undefined?{}:{stripeFields:clone(provider)})};}
  const mappers={[name+'Mapper']:mapper,['Create'+name+'Mapper']:{async toEntity(input,_em,provider){const row={entity:name,...mapped(input,provider)};identity.set(row.id,row);return row;}},['Update'+name+'Mapper']:{async toEntity(input,_em,provider){const row={...(em.table(name).get(input.id)??identity.get(input.id)??{}),entity:name,...mapped(input,provider)};identity.set(row.id,row);return row;}}};
  const telemetry={info(){},warn(){},error(){},log(){}};
  services[name]=['Plan','Subscription'].includes(name)?new wrappers[name](stripe,em,telemetry,{},mappers):new wrappers[name](stripe,em,cache,telemetry,{},mappers,{enableDatabaseBackup:backup});
 }
 const expected=[];
 for(const {service,method,input,expectNoProvider}of actions){const requestsBefore=captured.length;try{const result=service==='Inspect'?em.table(method).get(input.id)??null:await services[service][method](clone(input));if(result?.expiresAt&&service!=='Inspect'){assert.ok(Math.abs(new Date(result.expiresAt).getTime()-Date.now()-300000)<10000,'TS provider session expiry must remain five minutes');}expected.push({ok:normalize(result??null)});}catch{expected.push({error:true});}if(expectNoProvider){assert.equal(captured.length,requestsBefore);assert.deepEqual(expected.at(-1),{error:true});}}
 const officialRequests=captured;counts={};records=new Map();captured=[];
 const child=spawn(path.join(target,'debug/examples/services_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,backup,actions}));const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 const actual=JSON.parse(stdout);assert.equal(actual.length,expected.length);
 for(let i=0;i<actual.length;i++){const result=actual[i].ok;if(result?.expiresAt&&actions[i].service!=='Inspect'){assert.ok(Math.abs(new Date(result.expiresAt).getTime()-Date.now()-300000)<10000,'native provider session expiry must remain five minutes');}}
 for(let i=0;i<actual.length;i++){assert.deepEqual(normalize(nativeNames(actual[i])),expected[i],JSON.stringify({backup,index:i,action:actions[i]}));count++;}
 assert.deepEqual(captured,officialRequests,'provider effects must match actual TS service wrappers');
 }}finally{await new Promise(resolve=>server.close(resolve));}
console.log(`PASS ${count} actual TypeScript/Rust Stripe billing service operations over official SDK transport: plans, subscriptions, checkout, payment links, portal and cache/backup lifecycle`);
