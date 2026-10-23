// Actual pinned SDK, owned loopback only. Not a live provider acceptance claim.
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH??'stripe');
assert.equal(Stripe.PACKAGE_VERSION,'22.6.2');
const version='2026-08-26.dahlia',key='fl-checkout-00000000-0000-4000-8000-000000000001',calls=[];
const fields={mode:'subscription',ui_mode:'hosted_page',customer:'cus_owned',line_items:[{price:'custom_plan_owned',quantity:2}],payment_method_types:['card'],success_url:'https://example.test/success',cancel_url:'https://example.test/cancel',allow_promotion_codes:false,automatic_tax:{enabled:false},adaptive_pricing:{enabled:false}};
const expected={'mode':'subscription','ui_mode':'hosted_page','customer':'cus_owned','line_items[0][price]':'custom_plan_owned','line_items[0][quantity]':'2','payment_method_types[0]':'card','success_url':fields.success_url,'cancel_url':fields.cancel_url,'allow_promotion_codes':'false','automatic_tax[enabled]':'false','adaptive_pricing[enabled]':'false'};
let fail=false;
const server=http.createServer(async(req,res)=>{
 let body='';for await(const chunk of req)body+=chunk;
 assert.equal(req.headers.authorization,'Bearer owned-checkout-key');assert.equal(req.headers['stripe-version'],version);assert.equal(req.headers['stripe-account'],undefined);
 const u=new URL(req.url,'http://fixture');calls.push([req.method,u.pathname]);let result;
 if(req.method==='POST'){
  assert.equal(u.pathname,'/v1/checkout/sessions');assert.equal(req.headers['idempotency-key'],key);assert.deepEqual(Object.fromEntries(new URLSearchParams(body)),expected);
  if(fail){res.writeHead(500,{'content-type':'application/json'});res.end(JSON.stringify({error:{type:'api_error',message:'Owned uncertain response'}}));return;}
  result={id:'cs_owned',object:'checkout.session',customer:'cus_owned',livemode:false,mode:'subscription',ui_mode:'hosted_page',status:'open',payment_status:'unpaid',url:'https://checkout.stripe.com/c/pay/cs_owned'};
 }else{
  assert.equal(req.method,'GET');assert.equal(body,'');assert.equal(req.headers['idempotency-key'],undefined);
  result={'/v1/account':{id:'acct_owned',object:'account'},'/v1/balance':{object:'balance',livemode:false},'/v1/customers/cus_owned':{id:'cus_owned',object:'customer',livemode:false},'/v1/plans/custom_plan_owned':{id:'custom_plan_owned',object:'plan',product:{id:'prod_owned',object:'product'}},'/v1/prices/custom_plan_owned':{id:'custom_plan_owned',object:'price',livemode:false,type:'recurring'},'/v1/checkout/sessions/cs_owned/line_items':{object:'list',data:[{id:'li_owned',object:'item',quantity:2,price:{id:'custom_plan_owned',object:'price'}}],has_more:false,url:u.pathname}}[u.pathname];assert(result);
  if(u.pathname.includes('/plans/'))assert.equal(u.searchParams.get('expand[0]'),'product');
  if(u.pathname.endsWith('/line_items'))assert.equal(u.searchParams.get('limit'),'2');
 }
 res.writeHead(200,{'content-type':'application/json'});res.end(JSON.stringify(result));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const stripe=new Stripe('owned-checkout-key',{host:'127.0.0.1',port:server.address().port,protocol:'http',apiVersion:version,maxNetworkRetries:0});
 await stripe.accounts.retrieve();await stripe.balance.retrieve();await stripe.customers.retrieve('cus_owned');await stripe.plans.retrieve('custom_plan_owned',{expand:['product']});await stripe.prices.retrieve('custom_plan_owned');
 const session=await stripe.checkout.sessions.create(fields,{idempotencyKey:key});assert.equal(session.id,'cs_owned');
 const items=await stripe.checkout.sessions.listLineItems(session.id,{limit:2});assert.equal(items.has_more,false);assert.equal(items.data.length,1);
 assert.equal(calls.length,7);fail=true;await assert.rejects(stripe.checkout.sessions.create(fields,{idempotencyKey:key}));assert.equal(calls.length,8);
 console.log('PASS Stripe22.6.2 exact Checkout subscription/hosted/card form, custom Plan ID price lookup, bounded line-items GET, version/idempotency/no Connect, one failed POST; no live provider acceptance claim');
}finally{await new Promise(resolve=>server.close(resolve));}
