import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {realpath,readFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawn} from 'node:child_process';
import http from 'node:http';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const Stripe=require(process.env.STRIPE_SDK_PATH??require.resolve('stripe',{paths:[path.dirname(await realpath(tsc))]}));
const repository=JSON.parse(await readFile(path.join(root,'blueprint/implementations/billing/stripe/package.json'),'utf8'));assert.equal(repository.dependencies.stripe,`^${Stripe.PACKAGE_VERSION}`);
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','client_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
const actions=[
 {resource:'context',method:'verify',account:'acct_fixture',livemode:false},
 {resource:'customers',method:'create',fields:{email:'customer@example.test',name:'Customer 💬',address:{line1:'1 Test Street',country:'US'},metadata:{account:'local-account'}}},
 {resource:'customers',method:'retrieve',id:'cus_fixture',fields:{expand:['invoice_settings.default_payment_method']}},
 {resource:'customers',method:'update',id:'cus_fixture',fields:{name:null,metadata:{account:'updated-account'}}},
 {resource:'customers',method:'list',fields:{email:'customer@example.test',limit:10,starting_after:'cus_previous'}},
 {resource:'customers',method:'delete',id:'cus_fixture'},

 {resource:'plans',method:'create',fields:{amount:1999,interval:'month',currency:'USD',product:'prod_fixture',metadata:{note:'Unicode 💬 +&% /?'}}},
 {resource:'plans',method:'retrieve',id:'plan_fixture',fields:{expand:['product']}},
 {resource:'plans',method:'delete',id:'plan_fixture'},
 {resource:'subscriptions',method:'create',fields:{customer:'cus_fixture',items:[{plan:'plan_fixture',quantity:2}],trial_period_days:7}},
 {resource:'subscriptions',method:'retrieve',id:'sub_fixture'},
 {resource:'subscriptions',method:'update',id:'sub_fixture',fields:{items:[{id:'si_fixture',plan:'plan_other'}],metadata:{empty:null}}},
 {resource:'subscriptions',method:'delete',id:'sub_fixture'},
 {resource:'subscriptions',method:'resume',id:'sub_fixture'},
 {resource:'checkout',method:'create',fields:{mode:'subscription',payment_method_types:['card'],currency:'USD',success_url:'https://example.test/success?a=1&b=%',cancel_url:'https://example.test/cancel',line_items:[{price:'price_fixture',quantity:1}]}},
 {resource:'checkout',method:'retrieve',id:'cs_fixture'},
 {resource:'checkout',method:'update',id:'cs_fixture',fields:{metadata:{status:'SUCCESS'}}},
 {resource:'checkout',method:'expire',id:'cs_fixture'},
 {resource:'links',method:'create',fields:{payment_method_types:['card'],currency:'USD',line_items:[{price:'price_fixture',quantity:1}]}},
 {resource:'links',method:'update',id:'plink_fixture',fields:{metadata:{status:'EXPIRED'},active:false}},
 {resource:'links',method:'retrieve',id:'plink_fixture'},
 {resource:'links',method:'list',fields:{active:true}},
 {resource:'portal',method:'create',fields:{customer:'cus_fixture',return_url:'https://example.test/account',flow_data:{type:'subscription_cancel',subscription_cancel:{subscription:'sub_fixture'}}}},
 {resource:'plans',method:'retrieve',id:'provider π /?&'},
 {resource:'plans',method:'retrieve',id:'provider-failure'},
 {resource:'plans',method:'retrieve',id:'invalid-json'},
 {resource:'plans',method:'retrieve',id:'redirect'},
];
const captured=[];let redirected=0;
const server=http.createServer(async(request,response)=>{
 let raw='';for await(const chunk of request)raw+=chunk;
 const url=new URL(request.url,'http://fixture');
 if(url.pathname==='/must-not-follow'){redirected++;response.end('{}');return;}
 assert.equal(request.headers.authorization,'Bearer sk_fixture');assert.equal(request.headers['stripe-version'],Stripe.API_VERSION);
 const fields=[...new URLSearchParams(request.method==='GET'?url.search:raw)].sort(([a],[b])=>a.localeCompare(b));
 captured.push({method:request.method,path:url.pathname.split('/').map(decodeURIComponent),fields});
 if(url.pathname.endsWith('/provider-failure')){response.writeHead(500);response.end('{"error":{"type":"api_error","message":"sk_fixture private@example.test"}}');return;}
 if(url.pathname.endsWith('/invalid-json')){response.end('invalid-json');return;}
 if(url.pathname.endsWith('/redirect')){response.writeHead(302,{location:'/must-not-follow'});response.end('{}');return;}
 response.setHeader('content-type','application/json');
 if(url.pathname==='/v1/account'||url.pathname==='/v1/balance'){
  assert.equal(request.method,'GET');assert.equal(request.headers['stripe-account'],undefined);assert.equal(raw,'');
  response.end(JSON.stringify(url.pathname==='/v1/account'?{id:'acct_fixture',object:'account'}:{object:'balance',livemode:false}));return;
 }
 response.end(JSON.stringify(request.method==='GET'&&['/v1/customers','/v1/payment_links','/v1/plans','/v1/subscriptions'].includes(url.pathname)?{object:'list',data:[{id:'fixture'}],has_more:false}:{id:'fixture',object:'owned-fixture'}));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port,base=`http://127.0.0.1:${port}`;
const stripe=new Stripe('sk_fixture',{host:'127.0.0.1',port,protocol:'http',maxNetworkRetries:0,timeout:1000});
const resources={customers:stripe.customers,plans:stripe.plans,subscriptions:stripe.subscriptions,checkout:stripe.checkout.sessions,links:stripe.paymentLinks,portal:stripe.billingPortal.sessions};
try{
 const expected=[];
 for(const action of actions){try{
  if(action.resource==='context'){
   const account=await stripe.accounts.retrieve(),balance=await stripe.balance.retrieve();
   assert.equal(account.id,action.account);assert.equal(balance.livemode,action.livemode);
   expected.push({ok:{account:account.id,livemode:balance.livemode}});continue;
  }
  const api=resources[action.resource],method=action.resource==='subscriptions'&&action.method==='delete'?'cancel':['plans','customers'].includes(action.resource)&&action.method==='delete'?'del':action.method;
  const result=await api[method](...(['create','list'].includes(action.method)?[action.fields??{}]:action.method==='delete'||['expire','resume'].includes(action.method)?[action.id]:[action.id,action.fields??{}]));expected.push({ok:JSON.parse(JSON.stringify(result))});
 }catch{expected.push({error:true});}}
 const officialRequests=captured.splice(0);
 const child=spawn(path.join(metadata.target_directory,'debug/examples/client_parity'),[],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.stdin.end(JSON.stringify({base,actions}));
 const timer=setTimeout(()=>child.kill('SIGKILL'),30000);try{assert.equal(await new Promise(resolve=>child.once('close',resolve)),0,stderr);}finally{clearTimeout(timer);}
 const actual=JSON.parse(stdout);assert.deepEqual(actual.slice(0,-1).map(result=>result.error?{error:true}:result),expected.slice(0,-1));assert.ok(actual.at(-1).error,'native must refuse redirect responses even when the SDK accepts an empty object');assert.deepEqual(captured,officialRequests);assert.equal(redirected,0);
 for(const result of actual.filter(result=>result.error)){assert.ok(!JSON.stringify(result).includes('sk_fixture'));assert.ok(!JSON.stringify(result).includes('private@example.test'));}
 console.log(`PASS ${actions.length} official Stripe ${Stripe.PACKAGE_VERSION}/Rust billing client cases: versioned authenticated requests, nested form/query encoding, resource lifecycle, path isolation, failed/invalid responses and redirect refusal`);
}finally{await new Promise(resolve=>server.close(resolve));}
