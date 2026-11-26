// Installed official Stripe SDK, fixed read-only Customer request on owned loopback.
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH??'stripe');
let calls=0;
const server=http.createServer(async(req,res)=>{
 let body='';for await(const part of req)body+=part;
 assert.equal(req.method,'GET');assert.equal(req.url,'/v1/customers/custom-customer');assert.equal(body,'');
 assert.equal(req.headers.authorization,'Bearer opaque-customer-key');assert.equal(req.headers['stripe-version'],'2026-08-26.dahlia');assert.equal(req.headers['stripe-account'],undefined);assert.equal(req.headers['idempotency-key'],undefined);
 calls++;res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify(calls===1?{id:'custom-customer',object:'customer',livemode:false}:{id:'custom-customer',object:'customer',deleted:true}));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const client=new Stripe('opaque-customer-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:'2026-08-26.dahlia',maxNetworkRetries:0});
 assert.equal((await client.customers.retrieve('custom-customer')).livemode,false);
 assert.equal((await client.customers.retrieve('custom-customer')).deleted,true);
 assert.equal(calls,2);console.log('PASS official Stripe SDK fixed Customer GET/auth/version, no Connect/idempotency/body, and deleted-Customer variant requiring explicit rejection by native registration');
}finally{await new Promise(resolve=>server.close(resolve));}
