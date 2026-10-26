// Official pinned SDK wire oracle, owned loopback only: no live Portal creation.
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH??'stripe');
assert.equal(Stripe.PACKAGE_VERSION,'22.6.2');
const version='2026-08-26.dahlia',calls=[],key='fl-portal-00000000-0000-4000-8000-000000000001';
const fields={customer:'cus_owned',configuration:'bpc_owned',return_url:'https://example.test/account'};
let fail=false;
const server=http.createServer(async(req,res)=>{
 let body='';for await(const chunk of req)body+=chunk;
 assert.equal(req.headers.authorization,'Bearer owned-portal-key');assert.equal(req.headers['stripe-version'],version);assert.equal(req.headers['stripe-account'],undefined);
 calls.push([req.method,req.url]);let result;
 if(req.method==='POST'){
  assert.equal(req.url,'/v1/billing_portal/sessions');assert.equal(req.headers['idempotency-key'],key);assert.deepEqual(Object.fromEntries(new URLSearchParams(body)),fields);
  if(fail){res.writeHead(500,{'content-type':'application/json'});res.end(JSON.stringify({error:{type:'api_error',message:'Owned uncertain response'}}));return;}
  result={id:'bps_owned',object:'billing_portal.session',configuration:'bpc_owned',customer:'cus_owned',created:1791936000,livemode:false,flow:null,on_behalf_of:null,return_url:fields.return_url,url:'https://billing.stripe.com/p/session?secret=owned-private'};
 }else{
  assert.equal(req.method,'GET');assert.equal(body,'');assert.equal(req.headers['idempotency-key'],undefined);
  result={'/v1/account':{id:'acct_owned',object:'account'},'/v1/balance':{object:'balance',livemode:false},'/v1/customers/cus_owned':{id:'cus_owned',object:'customer',livemode:false},'/v1/billing_portal/configurations/bpc_owned':{id:'bpc_owned',object:'billing_portal.configuration',livemode:false,active:true}}[req.url];assert(result);
 }
 res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify(result));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const stripe=new Stripe('owned-portal-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:version,maxNetworkRetries:0});
 await stripe.accounts.retrieve();await stripe.balance.retrieve();await stripe.customers.retrieve('cus_owned');
 const configuration=await stripe.billingPortal.configurations.retrieve('bpc_owned');assert.equal(configuration.active,true);
 const session=await stripe.billingPortal.sessions.create(fields,{idempotencyKey:key});assert.equal(session.customer,'cus_owned');assert.equal(session.configuration,'bpc_owned');assert.equal(session.object,'billing_portal.session');
 assert.deepEqual(calls,[['GET','/v1/account'],['GET','/v1/balance'],['GET','/v1/customers/cus_owned'],['GET','/v1/billing_portal/configurations/bpc_owned'],['POST','/v1/billing_portal/sessions']]);
 fail=true;await assert.rejects(stripe.billingPortal.sessions.create(fields,{idempotencyKey:key}));assert.equal(calls.length,6);
 console.log('PASS Stripe22.6.2 Portal account/mode/customer/configuration GETs + exact session form/idempotency/version, no Connect, single failed POST with retries disabled; no live provider claim');
}finally{await new Promise(resolve=>server.close(resolve));}
