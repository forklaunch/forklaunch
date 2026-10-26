// Official Stripe SDK wire contract for the deliberately narrow bound-expiration profile.
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const Stripe=require(process.env.STRIPE_SDK_PATH??'stripe');
let calls=0;
const server=http.createServer(async(request,response)=>{
 const chunks=[];for await(const chunk of request)chunks.push(chunk);
 calls++;
 assert.equal(request.method,'POST');assert.equal(request.url,'/v1/payment_links/plink_owned');
 assert.equal(request.headers.authorization,'Bearer opaque-bound-expiration-key');
 assert.equal(request.headers['stripe-version'],'2026-08-26.dahlia');
 assert.equal(request.headers['idempotency-key'],'fl-plink-expire-00000000-0000-4000-8000-000000000001');
 assert.equal(request.headers['stripe-account'],undefined);
 const params=new URLSearchParams(Buffer.concat(chunks).toString());
 assert.deepEqual(Object.fromEntries(params),{'metadata[status]':'EXPIRED'});
 response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify({id:'plink_owned',object:'payment_link',livemode:false,active:true,metadata:{status:'EXPIRED'}}));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try {
 const stripe=new Stripe('opaque-bound-expiration-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:'2026-08-26.dahlia',maxNetworkRetries:0});
 const value=await stripe.paymentLinks.update('plink_owned',{metadata:{status:'EXPIRED'}},{idempotencyKey:'fl-plink-expire-00000000-0000-4000-8000-000000000001'});
 assert.equal(value.id,'plink_owned');assert.equal(value.active,true);assert.equal(value.metadata.status,'EXPIRED');assert.equal(calls,1);
 console.log('PASS official Stripe SDK bound-expiration wire: exact POST path, pinned version/auth/idempotency header, form values, no Connect header, no retry');
} finally {await new Promise(resolve=>server.close(resolve));}
