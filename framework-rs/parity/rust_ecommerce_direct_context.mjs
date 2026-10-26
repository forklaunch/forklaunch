// Official pinned Stripe SDK and native ecommerce read-only request comparison.
import assert from 'node:assert/strict';
import http from 'node:http';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {execFileSync,spawn} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
assert(process.env.STRIPE_SDK_PATH,'Set STRIPE_SDK_PATH to the installed official Stripe SDK');
const Stripe=createRequire(import.meta.url)(process.env.STRIPE_SDK_PATH);
const calls=[];let connected=false;
const server=http.createServer((req,res)=>{
 assert.equal(req.headers.authorization,'Bearer owned-context-secret');
 assert.equal(req.headers['stripe-account'],connected?'acct_owned':undefined);
 const url=new URL(req.url,'http://owned.invalid');
 const expansions=[...url.searchParams].map(([key,value])=>{assert.ok(key==='expand[]'||key==='expand[0]');return value;});
 calls.push({method:req.method,path:url.pathname,expansions});
 const value=req.url==='/v1/account'?{object:'account',id:'acct_owned'}:req.url==='/v1/balance'?{object:'balance',livemode:false}:url.pathname==='/v1/payment_intents/pi_owned'?{object:'payment_intent',id:'pi_owned',livemode:false}:null;
 res.setHeader('content-type','application/json');res.end(JSON.stringify(value));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const port=server.address().port,origin=`http://127.0.0.1:${port}`;
 if(process.env.FORKLAUNCH_SKIP_CARGO_BUILD!=='1')execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-stripe','--example','direct_context'],{cwd:root,stdio:'inherit'});
 for(const mode of [false,true]){
 connected=mode;calls.length=0;
 const sdk=new Stripe('owned-context-secret',{host:'127.0.0.1',port,protocol:'http',maxNetworkRetries:0});
 const options=connected?{stripeAccount:'acct_owned'}:undefined;
 const account=await sdk.accounts.retrieveCurrent({},options);const balance=await sdk.balance.retrieve({},options);const intent=await sdk.paymentIntents.retrieve('pi_owned',{expand:['latest_charge']},options);
 assert.equal(account.id,'acct_owned');assert.equal(balance.livemode,false);assert.equal(intent.id,'pi_owned');
 const expected=calls.splice(0);
 assert.deepEqual(expected,[{method:'GET',path:'/v1/account',expansions:[]},{method:'GET',path:'/v1/balance',expansions:[]},{method:'GET',path:'/v1/payment_intents/pi_owned',expansions:['latest_charge']}]);

 const result=await new Promise((resolve,reject)=>{const child=spawn(path.join(root,'blueprint-rs/target/debug/examples/direct_context'),[origin,...(connected?['connect']:[])],{stdio:['ignore','pipe','inherit']});let out='';child.stdout.on('data',chunk=>out+=chunk);child.on('error',reject);child.on('close',code=>code?reject(Error(`native exit ${code}`)):resolve(JSON.parse(out)));});
 assert.deepEqual(result,{account:'acct_owned',livemode:false,intent:'pi_owned'});assert.deepEqual(calls,expected);
 }
 console.log(`PASS official Stripe ${Stripe.PACKAGE_VERSION} / native ecommerce direct context: direct/Connect twins, three exact GET paths, bearer and exact account headers`);
}finally{await new Promise(resolve=>server.close(resolve));}
