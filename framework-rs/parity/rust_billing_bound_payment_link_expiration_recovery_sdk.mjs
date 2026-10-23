// Official SDK22.6.2 GET wire and declared event representation, no provider mutation.
// https://docs.stripe.com/api/events/object
// https://docs.stripe.com/api/events/retrieve (rendering version belongs to the event)
// https://docs.stripe.com/api/events/types (payment_link.updated)
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import path from 'node:path';
const require=createRequire(import.meta.url),sdkPath=process.env.STRIPE_SDK_PATH;
const Stripe=require(sdkPath??'stripe');
const types=readFileSync(path.join(sdkPath,'cjs/resources/Events.d.ts'),'utf8');
assert.match(types,/idempotency_key: string \| null/);
assert.match(types,/interface PaymentLinkUpdatedEvent/);
const link={id:'plink_owned',object:'payment_link',livemode:false,active:true,metadata:{status:'EXPIRED'}};
const event={id:'evt_owned',object:'event',type:'payment_link.updated',api_version:'2026-08-26.dahlia',created:1700000000,livemode:false,request:{id:'req_owned',idempotency_key:'fl-plink-expire-owned'},data:{object:link}};
const calls=[];
const server=http.createServer(async(request,response)=>{
 const chunks=[];for await(const chunk of request)chunks.push(chunk);
 assert.equal(request.method,'GET');assert.equal(Buffer.concat(chunks).length,0);
 assert.equal(request.headers.authorization,'Bearer owned-recovery-key');
 assert.equal(request.headers['stripe-version'],'2026-08-26.dahlia');
 assert.equal(request.headers['stripe-account'],undefined);assert.equal(request.headers['idempotency-key'],undefined);
 calls.push(request.url);assert(['/v1/events/evt_owned','/v1/payment_links/plink_owned'].includes(request.url));
 response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify(request.url.includes('/events/')?event:link));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try {
 const stripe=new Stripe('owned-recovery-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:'2026-08-26.dahlia',maxNetworkRetries:0});
 const observed=await stripe.events.retrieve('evt_owned');const current=await stripe.paymentLinks.retrieve('plink_owned');
 assert.equal(observed.request.idempotency_key,event.request.idempotency_key);assert.deepEqual(observed.data.object,current);assert.equal(current.active,true);
 assert.deepEqual(calls,['/v1/events/evt_owned','/v1/payment_links/plink_owned']);
 console.log('PASS official Stripe SDK22.6.2 event/payment-link GET wire, nullable request-key type, payment_link.updated representation, exact auth/version and zero POST/Connect/idempotency headers');
} finally {await new Promise(resolve=>server.close(resolve));}
