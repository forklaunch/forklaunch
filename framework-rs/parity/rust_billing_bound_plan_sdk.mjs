// Actual installed Stripe SDK; only owned loopback HTTP, no credentials/provider I/O.
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH??'stripe');
const fields={product:'custom-product',amount:1200,currency:'usd',interval:'month',interval_count:1,billing_scheme:'per_unit',usage_type:'licensed',active:true};
const expected=Object.fromEntries(Object.entries(fields).map(([k,v])=>[k,String(v)]));
const calls=[];
const server=http.createServer(async(req,res)=>{
 const chunks=[];for await(const c of req)chunks.push(c);
 assert.equal(req.headers.authorization,'Bearer opaque-bound-creation-key');assert.equal(req.headers['stripe-version'],'2026-08-26.dahlia');assert.equal(req.headers['stripe-account'],undefined);
 calls.push([req.method,req.url]);
 if(req.method==='POST'){
  assert.equal(req.url,'/v1/plans');assert.equal(req.headers['idempotency-key'],'fl-plan-00000000-0000-4000-8000-000000000001');assert.deepEqual(Object.fromEntries(new URLSearchParams(Buffer.concat(chunks).toString())),expected);
 }else assert.equal(req.headers['idempotency-key'],undefined);
 res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify(req.url.startsWith('/v1/products/')?{id:'custom-product',object:'product',livemode:false}:{id:'custom-plan',object:'plan',livemode:false,product:'custom-product'}));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const client=new Stripe('opaque-bound-creation-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:'2026-08-26.dahlia',maxNetworkRetries:0});
 await client.products.retrieve('custom-product');await client.plans.create(fields,{idempotencyKey:'fl-plan-00000000-0000-4000-8000-000000000001'});await client.plans.retrieve('custom-plan',{expand:['product']});
 assert.deepEqual(calls,[['GET','/v1/products/custom-product'],['POST','/v1/plans'],['GET','/v1/plans/custom-plan?expand[0]=product']]);
 console.log('PASS official Stripe SDK Plan product/readback paths, finite form values, custom IDs, pinned version/bearer/idempotency and no Connect');
}finally{await new Promise(resolve=>server.close(resolve));}
