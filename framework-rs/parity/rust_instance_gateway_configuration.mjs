import assert from 'node:assert/strict';import path from 'node:path';import {fileURLToPath} from 'node:url';import {execFileSync} from 'node:child_process';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),{load}=actualHttpSources(root),{instanceGatewaySettings}=load(path.join(root,'framework/core/src/http/instanceGateway.ts'));
const keys=['PLATFORM_GATEWAY_URL','INSTANCE_ID','INSTANCE_HMAC_KEY','FORKLAUNCH_LOCAL_GATEWAY_MOCK'],original=keys.map(key=>process.env[key]);
const settings={PLATFORM_GATEWAY_URL:'https://gateway.example.test/mount/',INSTANCE_ID:'instance_fixture',INSTANCE_HMAC_KEY:'instance-secret-fixture'};
const cases=[{},settings,{INSTANCE_HMAC_KEY:settings.INSTANCE_HMAC_KEY},{...settings,INSTANCE_ID:''},{...settings,INSTANCE_HMAC_KEY:''},
 {...settings,PLATFORM_GATEWAY_URL:'http://127.0.0.1:8080'},
 {...settings,PLATFORM_GATEWAY_URL:'http://gateway-mock:8080',FORKLAUNCH_LOCAL_GATEWAY_MOCK:'true'},
 {...settings,PLATFORM_GATEWAY_URL:'http://gateway-mock:8080///',FORKLAUNCH_LOCAL_GATEWAY_MOCK:'1'},
 {...settings,PLATFORM_GATEWAY_URL:'http://gateway-mock:8080',stronger:true},
 {...settings,PLATFORM_GATEWAY_URL:'http://gateway.example.test',FORKLAUNCH_LOCAL_GATEWAY_MOCK:'true',stronger:true},
 {...settings,FORKLAUNCH_LOCAL_GATEWAY_MOCK:'invalid',stronger:true},
 {...settings,INSTANCE_ID:'id with spaces',stronger:true},
 {...settings,PLATFORM_GATEWAY_URL:'https://user:secret@gateway.example.test',stronger:true},
 {...settings,FORKLAUNCH_LOCAL_GATEWAY_MOCK:'true'},
];
execFileSync('cargo',['build','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-billing-stripe','--example','gateway_configuration'],{cwd:root,stdio:'inherit',timeout:120000});
const metadata=JSON.parse(execFileSync('cargo',['metadata','--no-deps','--format-version','1','--manifest-path','blueprint-rs/Cargo.toml'],{cwd:root,encoding:'utf8'}));
try{
 for(const config of cases){
  for(const key of keys)delete process.env[key];for(const key of keys)if(config[key]!==undefined)process.env[key]=config[key];
  let actualTs;try{instanceGatewaySettings();actualTs={ok:true};}catch{actualTs={error:true};}
  const actual=JSON.parse(execFileSync(path.join(metadata.target_directory,'debug/examples/gateway_configuration'),{env:process.env,encoding:'utf8',timeout:10000}));
  if(config.stronger){assert.ok(actualTs.ok);assert.equal(actual.error,true);}else assert.deepEqual({...(actual.ok?{ok:true}:{error:true})},actualTs);
  assert.doesNotMatch(actual.description,/instance-secret-fixture|id with spaces|user:secret/);
 }
 console.log(`PASS ${cases.length} actual TypeScript/native managed configuration cases: missing/complete settings, loopback, explicit fixed Docker mock profile, remote TLS and credential/error redaction; unsafe URLs, invalid opt-in and IDs are rejected natively`);
}finally{for(let i=0;i<keys.length;i++){if(original[i]===undefined)delete process.env[keys[i]];else process.env[keys[i]]=original[i];}}
