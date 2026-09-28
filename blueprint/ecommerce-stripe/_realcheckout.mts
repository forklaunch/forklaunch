import { createHmacToken } from '@forklaunch/core/http';
import { randomUUID } from 'node:crypto';
const S='dev-hmac-secret-key-for-catalog-import-000', B='http://localhost:8001';
const A=(m:string,p:string,b?:unknown)=>{const t=new Date(),n=randomUUID();return `HMAC keyId=default ts=${t.toISOString()} nonce=${n} signature=${createHmacToken({method:m,path:p,body:b,timestamp:t,nonce:n,secretKey:S})}`;};
const call=async(m:string,u:string,sp:string,b?:unknown)=>{const r=await fetch(B+u,{method:m,headers:{'content-type':'application/json',authorization:A(m,sp,b)},body:b?JSON.stringify(b):undefined});const t=await r.text();let j:any;try{j=JSON.parse(t)}catch{j=t}return{code:r.status,body:j};};
// pick a real product + variant
let r=await call('GET','/product','/'); const prod=Array.isArray(r.body)?r.body.find((p:any)=>p.handle&&p.handle!=='hot-sauce'):null;
r=await call('GET',`/product/handle/${prod.handle}`,`/handle/${prod.handle}`); const pid=r.body.id;
r=await call('GET',`/variant/product/${pid}`,`/product/${pid}`); const vid=r.body[0].id;
console.log('product:',prod.handle,'variant:',vid);
r=await call('POST','/cart','/',{customerId:'demo-shopper'}); const cid=r.body.id;
await call('POST','/cart/items','/items',{cartId:cid,variantId:vid,quantity:1});
const addr={name:'Jane Buyer',line1:'500 Market St',city:'San Francisco',state:'CA',postalCode:'94105',country:'US'};
r=await call('POST','/checkout','/',{cartId:cid,provider:'stripe',shippingAddress:addr});
console.log('\nCHECKOUT: HTTP',r.code);
if(r.code===200){
  console.log('  ✅ REAL Stripe PaymentIntent created');
  console.log('  order id:', r.body.order?.id, '| status:', r.body.order?.status);
  console.log('  payment providerRef (Stripe PaymentIntent):', r.body.payment?.providerRef);
  console.log('  clientSecret present:', !!r.body.clientSecret);
} else {
  console.log('  body:', JSON.stringify(r.body).slice(0,300));
}
