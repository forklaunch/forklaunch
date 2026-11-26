import assert from 'node:assert/strict';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
import path from 'node:path';import {fileURLToPath} from 'node:url';import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),{load}=actualHttpSources(root);
const {createHmacToken}=load(path.join(root,'framework/core/src/http/createHmacToken.ts'));
const {verifyPlatformEvent}=load(path.join(root,'framework/core/src/http/instanceGateway.ts'));
const secret='instance-secret-fixture',now=Date.parse('2026-10-11T01:00:00.000Z'),payload={id:'event_fixture',feature:'email',type:'email.delivered',occurredAt:'2026-10-11T01:00:00Z',data:{messageId:'message_fixture',note:'Unicode 💬 +&%'}};
let sequence=0;
function action(options={}){
 const timestamp=options.timestamp??now,nonce=options.nonce??`nonce-${sequence++}`,method=options.method??'POST',path=options.path??'/platform-events/email',body=options.body??JSON.stringify(payload),keyId=options.keyId??'platform';
 const signature=createHmacToken({secretKey:options.secret??secret,method:method.toUpperCase(),path,body,timestamp:new Date(timestamp),nonce});
 return {method,path,body,now:options.now??now,authorization:`HMAC keyId=${keyId} ts=${new Date(timestamp).toISOString()} nonce=${nonce} signature=${signature}`,...(options.stronger?{stronger:true}:{})};
}
const repeated=action(),malformed=action({body:'invalid json'}),altered=action();
const actions=[
 action(),action({method:'post'}),action({body:' \n'+JSON.stringify(payload)+'\n'}),
 action({body:JSON.stringify({id:'minimal',feature:'sms',type:'sms.received'})}),
 action({body:JSON.stringify({...payload,extra:{preserved:true}})}),
 repeated,repeated,
 action({nonce:'other-delivery-same-event-id'}),
 action({timestamp:now-300000}),action({timestamp:now+300000}),
 action({timestamp:now-300001}),action({timestamp:now+300001}),
 action({keyId:'instance_fixture'}),action({secret:'wrong-secret'}),
 {...action(),authorization:''},{...action(),authorization:'Bearer token'},
 {...action(),path:'/platform-events/sms'}, {...action(),method:'GET'},
 {...altered,body:JSON.stringify({...payload,id:'tampered'})},altered,
 ...[null,[],42,{feature:'email',type:'event'},{id:42,feature:'email',type:'event'},{id:'id',feature:42,type:'event'},{id:'id',feature:'email',type:42}].map(value=>action({body:JSON.stringify(value)})),
 malformed,malformed,
 action({body:JSON.stringify({...payload,data:{large:'x'.repeat(1024*1024)}}),stronger:true}),
 // Actual TS expires a nonce five minutes after receipt, permitting reuse of a
 // still-valid future-dated signature. Native keeps it through signed expiry.
 action({timestamp:now+300000,nonce:'future-boundary'}),
 action({timestamp:now+300000,nonce:'future-window'}),
 action({timestamp:now+300000,nonce:'future-window',now:now+300001,stronger:true}),
 action({timestamp:now+300000,nonce:'future-boundary',now:now+600000,stronger:true}),
];
const expected=actions.map(action=>{try{return {ok:verifyPlatformEvent({method:action.method,path:action.path,headers:{authorization:action.authorization},body:action.body},{hmacKey:secret,now:()=>action.now})};}catch{return {error:true};}});
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','platform_events_parity'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
const actual=JSON.parse(execFileSync(path.join(metadata.target_directory,'debug/examples/platform_events_parity'),{input:JSON.stringify({actions,now}),encoding:'utf8',maxBuffer:4*1024*1024,timeout:30000}));
assert.equal(actual.results.length,actions.length);
for(let i=0;i<actions.length;i++){
 if(actions[i].stronger){assert.ok(expected[i].ok,'actual TypeScript accepts this documented stronger native boundary');assert.equal(actual.results[i].error,true);}
 else if(expected[i].error)assert.equal(actual.results[i].error,true);
 else assert.deepEqual(actual.results[i],JSON.parse(JSON.stringify(expected[i])));
}
assert.equal(actual.concurrentWinners,1);assert.equal(actual.boundedInclusiveExpiry,true);assert.equal(actual.redactedConfiguration,true);
for(const result of actual.results.filter(result=>result.error))assert.doesNotMatch(JSON.stringify(result),/instance-secret-fixture|message_fixture|Unicode/);
console.log(`PASS ${actions.length} actual TypeScript/native platform-event cases: exact bytes, shape, method/path/key binding, skew boundaries, replay, malformed delivery and event-ID redelivery; native additionally bounds bodies/nonces, rejects future-window replay and has one concurrent claim winner`);
