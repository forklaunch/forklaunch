import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import http from 'node:http';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');const require=createRequire(import.meta.url);const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc')))]}));
async function load(file,imports){const exports={};vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,file),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,fetch,Buffer,require(name){if(name in imports)return imports[name];throw Error('Unexpected runtime import '+name);}});return exports;}
const guards=await load('blueprint/implementations/ecommerce/paypal/domain/guards/paypal.guards.ts',{'@forklaunch/common':{isRecord:v=>v!==null&&typeof v==='object'&&!Array.isArray(v)}});
const {PaypalClient}=await load('blueprint/implementations/ecommerce/paypal/services/paypal-client.ts',{'../domain/guards/paypal.guards':guards});
const clone=v=>v===undefined?undefined:structuredClone(v);
class EntityManager {
 constructor(){this.rows=new Map();}async transactional(fn){return fn(this);}async persist(row){this.rows.set(row.id,row);}assign(row,update){Object.assign(row,update);}
 async findOneOrFail(_,where){const row=[...this.rows.values()].find(row=>Object.entries(where).every(([k,v])=>row[k]===v));if(!row)throw Error('not found');return row;}
 async nativeUpdate(_,where,update){const rows=[...this.rows.values()].filter(row=>Object.entries(where).every(([k,v])=>row[k]===v));for(const row of rows)Object.assign(row,update);return rows.length;}
}
const enums=await load('blueprint/interfaces/ecommerce/types/payment.service.types.ts',{});
const {BasePaymentService}=await load('blueprint/implementations/ecommerce/base/services/payment.service.ts',{'@forklaunch/core/http':{},'@mikro-orm/core':{EntityManager},'@forklaunch/interfaces-ecommerce/types':enums});
const {PaypalPaymentService}=await load('blueprint/implementations/ecommerce/paypal/services/payment.service.ts',{'@forklaunch/implementation-ecommerce-base/services':{BasePaymentService}});
const cases=[];for(const amountCents of [0,1,105,99999999,-105])cases.push({operation:'create',amountCents,currency:'usd',referenceId:'reference'});
for(const status of ['CREATED','SAVED','APPROVED','PAYER_ACTION_REQUIRED','VOIDED','COMPLETED','INVALID'])cases.push({operation:'create',amountCents:100,currency:'eur',referenceId:'status',responseStatus:status});
cases.push({operation:'capture',id:'ORDER_capture'});
for(const mode of ['success','failure','bad-shape','empty','http-failure','invalid-json'])cases.push({operation:'verify',params:{transmissionId:'transmission',transmissionTime:'2026-10-10T00:00:00Z',transmissionSig:'sig',certUrl:'http://127.0.0.1:9/cert',authAlgo:'SHA256withRSA',webhookId:'WH_fixture',webhookEvent:{mode}}});
for(const tokenMode of ['http-failure','bad-shape','invalid-json'])cases.push({operation:'capture',id:'ORDER_capture',tokenMode});
const actions=[];const add=(operation,input)=>actions.push({operation,input});
for(const id of ['normal','retry','failed']){add('create',{id,orderId:'ORDER_'+id,amountCents:105,currency:'usd'});if(id==='failed')add('fail',{providerRef:'ORDER_'+id});add('confirm',{providerRef:'ORDER_'+id});if(id==='retry')add('get',{id});add('confirm',{providerRef:'ORDER_'+id});add('fail',{providerRef:'ORDER_'+id});add('get',{id});}
add('confirm',{providerRef:'unknown'});add('get',{id:'unknown'});
let calls=[],captureCounts=new Map();
const server=http.createServer(async(req,res)=>{
 let raw='';for await(const chunk of req)raw+=chunk;const parts=req.url.split('/');const isCase=parts[1]==='case';const scenario=isCase?cases[Number(parts[2])]:null;const operationPath='/'+parts.slice(isCase?3:2).join('/');
 const body=raw?(req.headers['content-type']?.startsWith('application/json')?JSON.parse(raw):Object.fromEntries(new URLSearchParams(raw))):null;
 calls.push({method:req.method,url:req.url,authorization:req.headers.authorization,contentType:req.headers['content-type']??null,body});res.setHeader('content-type','application/json');
 if(operationPath==='/v1/oauth2/token'){if(scenario?.tokenMode==='http-failure'){res.statusCode=401;res.end('{}');}else if(scenario?.tokenMode==='bad-shape')res.end(JSON.stringify({access_token:42}));else if(scenario?.tokenMode==='invalid-json')res.end('{');else res.end(JSON.stringify({access_token:'owned-access-token'}));return;}
 if(operationPath==='/v2/checkout/orders'){res.end(JSON.stringify({id:isCase?'ORDER_client':body.purchase_units[0].reference_id,status:scenario?.responseStatus??'CREATED'}));return;}
 if(operationPath.endsWith('/capture')){const id=decodeURIComponent(operationPath.split('/').at(-2));const count=captureCounts.get(id)??0;captureCounts.set(id,count+1);if(!isCase&&id==='ORDER_retry'&&count===0){res.statusCode=503;res.end('{}');}else res.end(JSON.stringify({id,status:'COMPLETED'}));return;}
 if(operationPath==='/v1/notifications/verify-webhook-signature'){switch(body.webhook_event.mode){case'success':res.end(JSON.stringify({verification_status:'SUCCESS'}));break;case'failure':res.end(JSON.stringify({verification_status:'FAILURE'}));break;case'bad-shape':res.end(JSON.stringify({verification_status:42}));break;case'empty':res.end('{}');break;case'http-failure':res.statusCode=400;res.end('{}');break;default:res.end('{');}return;}
 res.statusCode=404;res.end('{}');
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
try {
 const expectedClient=[];for(let index=0;index<cases.length;index++){const c=cases[index];const client=new PaypalClient({clientId:'owned-client',clientSecret:'owned-secret',baseUrl:`${origin}/case/${index}`});try{expectedClient.push({ok:await(c.operation==='create'?client.createOrder(c):c.operation==='capture'?client.captureOrder(c.id):client.verifyWebhookSignature(c.params))});}catch{expectedClient.push({error:true});}}
 const em=new EntityManager();const mappers={CreatePaymentMapper:{async toEntity(input,_,order){return {...clone(input),status:'pending',providerRef:order.id};}},PaymentMapper:{entity:'Payment',async toDto(row){return clone(row);}}};
 const service=new PaypalPaymentService(new PaypalClient({clientId:'owned-client',clientSecret:'owned-secret',baseUrl:`${origin}/service`}),em,{}, {},mappers);
 const expectedService=[];for(const action of actions){const method={create:'createPayment',confirm:'confirmPayment',fail:'failPayment',get:'getPayment'}[action.operation];try{expectedService.push({ok:await service[method](action.input)});}catch{expectedService.push({error:true});}}
 const expectedCalls=calls;calls=[];captureCounts=new Map();
 const child=spawn('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-paypal','--example','paypal_parity',origin],{cwd:root,stdio:['pipe','pipe','pipe']});let output='',errors='';child.stdout.on('data',d=>output+=d);child.stderr.on('data',d=>errors+=d);child.stdin.end(JSON.stringify({client:cases,service:actions}));const timer=setTimeout(()=>child.kill('SIGKILL'),120000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,errors);}finally{clearTimeout(timer);}
 assert.deepEqual(JSON.parse(output),JSON.parse(JSON.stringify({client:expectedClient,service:expectedService})));assert.deepEqual(calls,expectedCalls);
 console.log(`PASS ${cases.length} actual TypeScript/Rust PayPal client cases and ${actions.length} payment wrapper operations: OAuth, Orders wire format/status guards, webhook verification, single capture, retry rollback, duplicates and lifecycle`);
}finally{await new Promise(resolve=>server.close(resolve));}
