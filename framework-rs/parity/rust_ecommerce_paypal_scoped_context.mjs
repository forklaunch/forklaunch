// Pinned primary PayPal account_id schema versus native validation and owned OAuth/order wire.
import assert from 'node:assert/strict';
import http from 'node:http';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {spawn} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const fixture=JSON.parse(readFileSync(path.join(root,'framework-rs/parity/fixtures/paypal_scoped_payee_schema.json')));
assert.equal(fixture.payeeMerchantId.allOf[0].$ref,'#/components/schemas/account_id');
const history=JSON.parse(readFileSync(path.join(root,'framework-rs/parity/fixtures/paypal_void_history_schema.json')));
assert.match(history.purchase_unit_payments.allOf[1].description,/comprehensive history/);
assert.equal(history.order_status.oneOf.find(v=>v.enum[0]==='VOIDED').description,'All purchase units in the order are voided.');
for(const key of ['captures','authorizations','refunds']){
 assert.equal(history.payment_collection.properties[key].type,'array');
 assert.equal(history.payment_collection.properties[key].minItems,0);
 assert(!history.payment_collection.required?.includes(key)); // Missing is not explicit empty evidence.
}
const schema=fixture.account_id,pattern=new RegExp(schema.pattern);
const valid=value=>value.length>=schema.minLength&&value.length<=schema.maxLength&&pattern.test(value);
const cases=Array.from({length:128},(_,byte)=>({merchant:String.fromCharCode(byte)+'7C9N6S4YRQXG'}));
cases.push(...['Q7C9N6S4YRQXG','J8NCBBULP5JH2','','Q7C9N6S4YRQXG0','merchant@example.invalid','Ｑ7C9N6S4YRQXG'].map(merchant=>({merchant})));
for(const fault of ['missing-payee','email-only','wrong-payee','multiunit','wrong-id','bad-token','wrong-token-type','token-denied','read-denied'])cases.push({merchant:'Q7C9N6S4YRQXG',fault});
for(const status of history.order_status.enum)cases.push({merchant:'Q7C9N6S4YRQXG',status});
for(const history of [{captures:[],authorizations:[],refunds:[]},{captures:null},{refunds:[{id:'prior'}]}])cases.push({merchant:'Q7C9N6S4YRQXG',status:'VOIDED',history});
const calls=[];
const server=http.createServer(async(req,res)=>{
 const [,prefix,indexText,...parts]=req.url.split('/');assert.equal(prefix,'case');const index=Number(indexText),c=cases[index],route='/'+parts.join('/');
 let body='';for await(const chunk of req)body+=chunk;
 assert.equal(req.headers['paypal-auth-assertion'],undefined);assert.equal(req.headers['paypal-partner-attribution-id'],undefined);
 calls.push({index,method:req.method,path:route});res.setHeader('content-type','application/json');
 if(route==='/v1/oauth2/token'){
  assert.equal(req.method,'POST');assert.equal(req.headers.authorization,'Basic '+Buffer.from('owned-client:owned-secret').toString('base64'));assert.equal(body,'grant_type=client_credentials');
  if(c.fault==='token-denied')res.statusCode=401;
  res.end(JSON.stringify({access_token:c.fault==='bad-token'?null:'owned-token',token_type:c.fault==='wrong-token-type'?'Basic':'Bearer',expires_in:300,app_id:'not-merchant-proof'}));return;
 }
 assert.equal(route,'/v2/checkout/orders/ORDER_owned');assert.equal(req.method,'GET');assert.equal(req.headers.authorization,'Bearer owned-token');assert.equal(body,'');
 if(c.fault==='read-denied')res.statusCode=403;
 const order={id:'ORDER_owned',status:'COMPLETED',intent:'CAPTURE',purchase_units:[{reference_id:'local-order',payee:{merchant_id:c.merchant,email_address:'private@example.invalid'},amount:{currency_code:'USD',value:'1.05'}}],payer:{private:'never-return'}};
 if(c.status)order.status=c.status;
 if(c.history)order.purchase_units[0].payments=c.history;
 if(c.fault==='missing-payee')delete order.purchase_units[0].payee;
 if(c.fault==='email-only')delete order.purchase_units[0].payee.merchant_id;
 if(c.fault==='wrong-payee')order.purchase_units[0].payee.merchant_id='J8NCBBULP5JH2';
 if(c.fault==='multiunit')order.purchase_units.push(order.purchase_units[0]);
 if(c.fault==='wrong-id')order.id='OTHER';res.end(JSON.stringify(order));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const origin=`http://127.0.0.1:${server.address().port}`;
 const child=spawn('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-paypal','--example','scoped_order_context',origin],{cwd:root,stdio:['pipe','pipe','pipe']});
 let output='',errors='';child.stdout.on('data',v=>output+=v);child.stderr.on('data',v=>errors+=v);child.stdin.end(JSON.stringify(cases));
 assert.equal(await new Promise(resolve=>child.on('close',resolve)),0,errors);
 const results=JSON.parse(output);assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++){
  const c=cases[index],expected=valid(c.merchant)&&!c.fault,result=results[index],wire=calls.filter(v=>v.index===index);
  assert.equal(Boolean(result.ok),expected,JSON.stringify(c));
  if(!valid(c.merchant)){assert.deepEqual(wire,[]);continue;}
  const tokenFailure=['bad-token','wrong-token-type','token-denied'].includes(c.fault);
  assert.deepEqual(wire.map(({method,path})=>({method,path})),[{method:'POST',path:'/v1/oauth2/token'},...tokenFailure?[]:[{method:'GET',path:'/v2/checkout/orders/ORDER_owned'}]]);
  if(expected)assert.deepEqual(result.ok,{providerRef:'ORDER_owned',status:c.status??'COMPLETED',amountCents:105,currency:'USD',environment:'sandbox'});
 }
 assert(!output.includes('never-return'));assert(!output.includes('private@example.invalid'));
 console.log(`PASS ${cases.length} primary-schema/native PayPal merchant cases plus exact OAuth/order wire, zero-I/O invalid identities, redaction and denied/malformed resource checks`);
}finally{await new Promise(resolve=>server.close(resolve));}
