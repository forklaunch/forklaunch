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
const exports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'blueprint/implementations/billing/stripe/services/webhook.service.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Date,require(spec){if(spec.includes('billingProvider.enum'))return{BillingProviderEnum:{STRIPE:'stripe'}};throw Error('Unexpected webhook import '+spec);}});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','webhook_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const target=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'})).target_directory;
let captured=[];
const server=http.createServer(async(request,response)=>{
 let raw='';for await(const chunk of request)raw+=chunk;
 const url=new URL(request.url,'http://fixture');const fields=new URLSearchParams(request.method==='GET'?url.search:raw);
 assert.equal(request.headers.authorization,'Bearer sk_fixture');assert.equal(request.headers['stripe-version'],Stripe.API_VERSION);
 captured.push({method:request.method,path:url.pathname,fields:[...fields].sort(([a],[b])=>a.localeCompare(b))});
 let value;
 if(url.pathname==='/v1/products/prod_fixture') value={id:'prod_fixture',active:true,name:'Fixture',metadata:{features:'["one",3,"two"]'}};
 else if(['/v1/plans','/v1/prices'].includes(url.pathname)){
  const price=url.pathname.endsWith('prices'),cursor=fields.get('starting_after');
  value=cursor?{object:'list',data:[{id:'missing',active:true}],has_more:false}:{object:'list',data:[{id:price?'price1':'plan1',active:false}],has_more:true};
 }else if(request.method==='DELETE')value={id:url.pathname.split('/').at(-1),deleted:true};
 else if(url.pathname.startsWith('/v1/checkout/sessions/'))value={id:url.pathname.split('/').at(-1)};
 else{response.writeHead(404);response.end('{}');return;}
 response.setHeader('content-type','application/json');response.end(JSON.stringify(value));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port,base='http://127.0.0.1:'+port;
const cachePrefix={Plan:'plan',Subscription:'subscription',CheckoutSession:'checkout_session',PaymentLink:'payment_link',BillingPortal:'billing_portal_session'};
const actions=[],event=(type,object)=>actions.push({id:'evt_'+actions.length,type,data:{object}});
const plan={id:'plan1',product:'prod_fixture',interval:'month',currency:'USD',amount:1999};
event('plan.created',plan);event('plan.updated',{...plan,amount:0});
const price={id:'price1',product:{id:'prod_fixture'},recurring:{interval:'year'},currency:'USD',unit_amount:2999,active:true};
event('price.created',price);event('price.updated',{...price,active:false});
event('price.created',{...price,id:'price_nonrecurring',recurring:null});
event('plan.created',{id:'invalid',amount:null});
event('product.updated',{id:'prod_fixture',active:true,name:'Changed',metadata:{features:'csv, two, , three'}});
event('product.created',{id:'prod_fixture',active:false,name:'Disabled',metadata:{features:'["json",null,"only"]'}});
const sub={id:'sub1',customer:{id:'cus_fixture'},items:{data:[{plan:{id:'plan1'}}]},created:1700000000,status:'incomplete',description:'Initial',cancel_at:1700000300};
event('customer.subscription.created',sub);
event('customer.subscription.updated',{...sub,customer:'must-not-reassign',status:'trialing',description:null,cancel_at:null});
event('customer.subscription.paused',{id:'sub1'});event('customer.subscription.resumed',{id:'sub1'});
event('customer.subscription.updated',{...sub,status:'past_due'});
event('customer.subscription.deleted',{id:'sub1'});event('customer.subscription.deleted',{id:'missing'});
event('customer.subscription.updated',{...sub,id:'unseen',customer:'cus_new',status:'active'});
event('customer.subscription.updated',{id:'invalid',items:{data:[]}});
event('billing_portal.session.created',{id:'bps_fixture',customer:'cus_fixture',created:1700000000,url:'https://provider.example/portal'});
const invalidPortalIds=new Set();
for(const [i,created] of [undefined,null,false,true,'1700000000',-1,1.5,253402300500,Number.MAX_SAFE_INTEGER,1e30,{},[]].entries()){
 const id='bps_invalid_'+i;invalidPortalIds.add(id);
 event('billing_portal.session.created',{id,customer:'cus_fixture',created,url:'https://provider.example/portal'});
}
for(const [id,created] of [['bps_epoch',0],['bps_upper',253402300499]])event('billing_portal.session.created',{id,customer:'cus_fixture',created,url:'https://provider.example/portal'});
const portalNumberSpellings={bps_decimal:'1700000000.0',bps_exponent:'1e3',bps_negative_zero:'-0.0'};
for(const [id,spelling] of Object.entries(portalNumberSpellings))event('billing_portal.session.created',{id,customer:'cus_fixture',created:JSON.parse(spelling),url:'https://provider.example/portal'});
event('payment_link.created',{id:'plink_fixture',currency:'USD',payment_method_types:['card'],line_items:{data:[{amount_total:20},{amount_total:30}]}});
event('payment_link.updated',{id:'plink_fixture',currency:'USD',payment_method_types:['card']});
for(const [i,type] of ['checkout.session.expired','checkout.session.completed'].entries()) {
 const id='cs_'+i;actions.push({seed:{kind:'CheckoutSession',data:{id,customerId:'cus_fixture',paymentMethods:['card'],currency:'USD',expiresAt:'2030-01-01T00:00:00.000Z',status:'PENDING'}}});event(type,{id});
}
event('plan.deleted',{id:'plan1'});event('unknown.event',{id:'noop'});
function normalize(value){return JSON.parse(JSON.stringify(value,(key,item)=>['createdAt','updatedAt','entity','providerFields','stripeFields'].includes(key)?undefined:key==='id'&&typeof item==='string'&&(/^[0-9a-f-]{36}$/.test(item)||item==='unseen')?'generated-local-id':['expiresAt','startDate','endDate'].includes(key)?new Date(item).toISOString():item));}
try {
 const em=new EntityManager(),cache=new Cache(),services={},stripe=new Stripe('sk_fixture',{host:'127.0.0.1',port,protocol:'http',maxNetworkRetries:0,timeout:1000});
 for(const name of names){const identity=new Map();
  const mapper={entity:name,async toDto(row){const{entity,...dto}=row;return clone(dto);}};
  function mapped(input,provider){const{stripeFields,...raw}=clone(input);return{...Object.fromEntries(Object.entries(raw).filter(([,v])=>v!==undefined)),...(provider===undefined?{}:{stripeFields:clone(provider)})};}
  const mappers={[name+'Mapper']:mapper,['Create'+name+'Mapper']:{async toEntity(input,_em,provider){const row={entity:name,...mapped(input,provider),id:input.id??'unseen'};identity.set(row.id,row);return row;}},['Update'+name+'Mapper']:{async toEntity(input,_em,provider){const existing=em.table(name).get(input.id)??identity.get(input.id);if(!existing)throw Error('Cannot update missing required billing row');const row={...existing,entity:name,...mapped(input,provider)};identity.set(row.id,row);return row;}}};
  const telemetry={info(){},warn(){},error(){}};
  services[name]=['Plan','Subscription'].includes(name)?new wrappers[name](stripe,em,telemetry,{},mappers):new wrappers[name](stripe,em,cache,telemetry,{},mappers,{enableDatabaseBackup:true});
 }
 const ledger=new Map(),receiptEm={async findOne(_entity,where){return ledger.get(where.stripeId)??null;},create(_entity,data){ledger.set(data.stripeId,data);},async flush(){}};
 const telemetry={info(){},warn(){},error(){}};
 const webhook=new exports.StripeWebhookService(stripe,receiptEm,{},telemetry,services.BillingPortal,services.CheckoutSession,services.PaymentLink,services.Plan,services.Subscription,{USER:'USER',ORGANIZATION:'ORGANIZATION'},'Webhook');
 const expected=[];
 for(const action of actions){let error=false;try{if(action.seed){const {kind,data}=action.seed;em.persist({entity:kind,...clone(data)});cache.rows.set(cachePrefix[kind]+':'+data.id,clone(data));}else await webhook.handleWebhookEvent(clone(action));}catch{error=true;}
 const state={};for(const name of names){state[name+':durable']=[...em.table(name).values()];state[name+':cache']=[...cache.rows].filter(([key])=>key.startsWith(cachePrefix[name]+':')).map(([,value])=>value);}
 if(action.type==='billing_portal.session.created'){
  const object=action.data.object,rows=state['BillingPortal:durable'];
  if(invalidPortalIds.has(object.id)){
   assert.equal(error,true,'invalid provider timestamp must fail before writing');
   assert.equal(rows.some(row=>row.id===object.id),false);
   assert.equal(state['BillingPortal:cache'].some(row=>row.id===object.id),false);
   assert.equal(ledger.has(action.id),false,'invalid event must not gain a receipt');
  }else{
   const wanted={bps_fixture:'2023-11-14T22:18:20.000Z',bps_epoch:'1970-01-01T00:05:00.000Z',bps_upper:'9999-12-31T23:59:59.000Z',bps_decimal:'2023-11-14T22:18:20.000Z',bps_exponent:'1970-01-01T00:21:40.000Z',bps_negative_zero:'1970-01-01T00:05:00.000Z'}[object.id];
   assert.equal(new Date(rows.find(row=>row.id===object.id).expiresAt).toISOString(),wanted);
  }
 }
 if(['payment_link.created','payment_link.updated'].includes(action.type)){
  assert.equal(state['PaymentLink:durable'].find(row=>row.id===action.data.object.id).status,action.type.endsWith('created')?'CREATED':'UPDATED');
 }
 if(action.type==='checkout.session.completed')assert.equal(state['CheckoutSession:durable'].find(row=>row.id===action.data.object.id).status,'SUCCESS');
 expected.push(normalize({error,state}));}
 const official=captured;captured=[];
 // Preserve selected JSON numeric lexemes: JSON.stringify would otherwise hide
 // whole-valued float/exponent differences between TS Number and serde_json.
 const wireActions=actions.map(action=>{
  const raw=JSON.stringify(action),spelling=portalNumberSpellings[action.data?.object?.id];
  return spelling===undefined?raw:raw.replace(/"created":[^,}]+/,`"created":${spelling}`);
 });
 const child=spawn(path.join(target,'debug/examples/webhook_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',v=>stdout+=v);child.stderr.on('data',v=>stderr+=v);child.stdin.end(`{"base":${JSON.stringify(base)},"actions":[${wireActions.join(',')}]}`);
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 const actual=normalize(JSON.parse(stdout));assert.equal(actual.length,expected.length);
 for(let i=0;i<actual.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify({index:i,action:actions[i]}));
 assert.deepEqual(captured,official,'Stripe provider effects must match actual TS webhook service');
 assert.equal(ledger.has(actions.find(v=>v.type==='plan.created'&&v.data.object.id==='invalid').id),false);
 console.log(`PASS ${actions.length} actual TS/Rust billing webhook states and provider effects, including product pagination, subscription reassignment protection and cache/backup lifecycle`);
} finally {await new Promise(resolve=>server.close(resolve));}
