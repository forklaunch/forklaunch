import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {realpathSync} from 'node:fs';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const Stripe=require(process.env.STRIPE_SDK_PATH??require.resolve('stripe',{paths:[path.dirname(realpathSync(tsc))]}));
const {load}=actualHttpSources(root);
const {stripeGatewayFetch}=load(path.join(root,'framework/core/src/http/paymentsClient.ts'));
const {createHmacToken}=load(path.join(root,'framework/core/src/http/createHmacToken.ts'));
const settings={gatewayUrl:'https://gateway.example.test:8443/prefix/',instanceId:'capability-fixture',hmacKey:'capability-only-key'};
const calls=[],preconnects=[],nonces=new Set();
async function transport(url,options){
 calls.push({url,options});
 const target=new URL(url);
 assert.equal(target.origin,'https://gateway.example.test:8443');
 assert.ok(target.pathname.startsWith('/prefix/instance-gateway/stripe/'));
 const route=target.pathname.slice('/prefix/instance-gateway'.length)+target.search;
 const match=/^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(options.headers.get('authorization'));
 assert.ok(match);assert.equal(match[1],settings.instanceId);assert.ok(!nonces.has(match[3]));nonces.add(match[3]);
 assert.equal(match[4],createHmacToken({method:options.method,path:route,body:options.body||undefined,timestamp:new Date(match[2]),nonce:match[3],secretKey:settings.hmacKey}));
 return new Response(JSON.stringify({object:'list',data:[],has_more:false}),{headers:{'content-type':'application/json'}});
}
const nodeAdapter=stripeGatewayFetch(settings,transport);
assert.equal(Object.hasOwn(nodeAdapter,'preconnect'),false);
const sentinel={result:true};
transport.preconnect=function(target,options){assert.equal(this,transport);preconnects.push({target,options});return sentinel;};
const bunAdapter=stripeGatewayFetch(settings,transport);
assert.equal(typeof bunAdapter.preconnect,'function');
const options={dns:true,tcp:false,tls:true};
assert.equal(bunAdapter.preconnect('https://api.stripe.com/private',options),sentinel);
assert.equal(preconnects[0].target,'https://gateway.example.test:8443');
assert.equal(preconnects[0].options,options);
bunAdapter.preconnect(new URL('https://untrusted.example.test/'));
assert.equal(preconnects[1].target,'https://gateway.example.test:8443');assert.equal(preconnects[1].options,undefined);
for(const adapter of [nodeAdapter,bunAdapter]){
 await adapter('https://api.stripe.com/v1/customers?expand[]=data',{method:'POST',body:'name=Unicode+%F0%9F%92%AC',headers:{authorization:'Bearer must-be-replaced'}});
 const sdk=new Stripe('fixture-not-a-provider-key',{httpClient:Stripe.createFetchHttpClient(adapter),maxNetworkRetries:0});
 assert.deepEqual((await sdk.customers.list({limit:1})).data,[]);
}
assert.equal(preconnects.length,2,'actual Stripe SDK calls only the fetch callable');
assert.equal(calls.length,4);assert.equal(nonces.size,4);
transport.preconnect='not callable';
assert.equal(Object.hasOwn(stripeGatewayFetch(settings,transport),'preconnect'),false);
assert.equal(Object.hasOwn(stripeGatewayFetch(settings),'preconnect'),typeof fetch.preconnect==='function');
console.log('PASS actual payments source Node/Bun conditional capability, gateway-only origin, options/receiver/return forwarding, 4 HMAC requests and actual Stripe SDK callable compatibility');
