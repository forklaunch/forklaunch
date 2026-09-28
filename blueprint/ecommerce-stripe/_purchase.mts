import { createHmacToken } from '@forklaunch/core/http';
import { randomUUID } from 'node:crypto';
const SECRET='dev-hmac-secret-key-for-catalog-import-000';
const BASE='http://localhost:8001';
function auth(method:string, path:string, body?:unknown){
  const timestamp=new Date(), nonce=randomUUID();
  const signature=createHmacToken({method,path,body,timestamp,nonce,secretKey:SECRET});
  return `HMAC keyId=default ts=${timestamp.toISOString()} nonce=${nonce} signature=${signature}`;
}
async function call(method:string, url:string, signedPath:string, body?:unknown){
  const res=await fetch(BASE+url,{method,headers:{'content-type':'application/json',authorization:auth(method,signedPath,body)},body:body?JSON.stringify(body):undefined});
  const t=await res.text(); let j:any; try{j=JSON.parse(t)}catch{j=t}
  return {code:res.status, body:j};
}
const handle='hot-sauce';
let r=await call('GET',`/product/handle/${handle}`,`/handle/${handle}`);
console.log('browse product:',r.code, 'id='+r.body?.id);
const productId=r.body.id;
r=await call('GET',`/variant/product/${productId}`,`/product/${productId}`);
const variantId=Array.isArray(r.body)?r.body[0]?.id:undefined;
console.log('list variants:',r.code,'variantId='+variantId);
r=await call('POST','/cart','/',{customerId:'e2e-customer-1'});
console.log('create cart:',r.code,'cartId='+r.body?.id);
const cartId=r.body.id;
r=await call('POST','/cart/items','/items',{cartId,variantId,quantity:2});
console.log('add to cart:',r.code, r.code===200?'OK':JSON.stringify(r.body).slice(0,120));
const orderBody={customerId:'e2e-customer-1',items:[{variantId,quantity:2,unitPriceCents:899}],shippingAddress:{name:'E2E',line1:'123 Test St',city:'Testville',state:'CA',postalCode:'90001',country:'US'},subtotalCents:1798,discountCents:0,taxCents:144,taxBreakdown:[{jurisdiction:'CA',taxCents:144}],shippingCents:0,giftCardCents:0,totalCents:1942};
r=await call('POST','/order','/',orderBody);
console.log('create order:',r.code,'status='+r.body?.status,'id='+r.body?.id);
if(r.code!==200){console.log('  order err:',JSON.stringify(r.body).slice(0,200));}
const orderId=r.body?.id;
for(const to of ['paid','fulfilled','shipped','delivered']){
  r=await call('PUT',`/order/${orderId}/transition`,`/${orderId}/transition`,{to});
  console.log(`  -> ${to}:`,r.code,'status='+r.body?.status);
}
r=await call('PUT',`/order/${orderId}/transition`,`/${orderId}/transition`,{to:'paid'});
console.log('illegal transition (want 400):',r.code);
r=await call('GET',`/order/${orderId}`,`/${orderId}`);
console.log('FINAL order status:',r.body?.status);
console.log(r.body?.status==='delivered'?'\n✅ REAL END-TO-END PURCHASE ON FORKLAUNCH — import→browse→cart→order→paid→fulfilled→shipped→delivered':'\n❌ incomplete');
