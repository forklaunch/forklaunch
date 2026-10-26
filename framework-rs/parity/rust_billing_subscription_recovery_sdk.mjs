// Owned loopback serialization oracle against Stripe 22.6.2; not live event proof.
// Event fields: https://docs.stripe.com/api/events/object
// Request-key reuse boundary: https://docs.stripe.com/api/idempotent_requests
import assert from 'node:assert/strict';
import http from 'node:http';
import { createRequire } from 'node:module';
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH);
assert.equal(Stripe.PACKAGE_VERSION,'22.6.2');
const version='2026-08-26.dahlia';
const idempotency='fl-subscription-00000000-0000-4000-8000-000000000001';
const base={id:'evt_owned',object:'event',api_version:version,created:1791936000,livemode:false,type:'customer.subscription.created',data:{object:{id:'sub_owned',object:'subscription',created:1791936000,customer:'cus_owned',livemode:false}},request:{id:'req_owned',idempotency_key:idempotency}};
let response=base;
let calls=0;
const server=http.createServer(async(req,res)=>{
 assert.equal(req.method,'GET');assert.equal(req.url,'/v1/events/evt_owned');
 assert.equal(req.headers.authorization,'Bearer owned-recovery-key');
 assert.equal(req.headers['stripe-version'],version);
 assert.equal(req.headers['stripe-account'],undefined);
 assert.equal(req.headers['idempotency-key'],undefined);
 let body='';for await(const c of req)body+=c;assert.equal(body,'');calls++;
 res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify(response));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
try{
 const client=new Stripe('owned-recovery-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:version,maxNetworkRetries:0});
 for(const request of [base.request,null,{id:null,idempotency_key:null},{id:'req_other',idempotency_key:'old-reused-key'}]){
  response={...base,request};const event=await client.events.retrieve('evt_owned');assert.deepEqual(event.request,request);assert.equal(event.data.object.id,'sub_owned');
 }
 response={...base,api_version:'2019-02-19'};assert.equal((await client.events.retrieve('evt_owned')).api_version,'2019-02-19');
 assert.equal(calls,5);
 console.log('Stripe22.6.2 Event GET: 5 wire/nullable-request/version-preservation cases PASS; fixture semantics only, no live causality claim');
}finally{await new Promise(r=>server.close(r));}
