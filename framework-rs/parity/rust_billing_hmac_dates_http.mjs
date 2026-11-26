// Build once, then run actual HTTP/provider-counter/SDK checks with fresh TZ processes.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:process.env.CARGO_BUILD_JOBS??'1',CARGO_TARGET_DIR:path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target'))};
const built=spawnSync('cargo',['test','--manifest-path',path.join(root,'blueprint-rs/Cargo.toml'),'-p','forklaunch-billing-stripe','--all-features','--test','http_configured_payment_link_update','--no-run','--message-format=json'],{cwd:root,env,encoding:'utf8',timeout:600000,maxBuffer:32<<20});
const messages=built.stdout.split('\n').flatMap(line=>{try{return[JSON.parse(line)]}catch{return[]}});
assert.equal(built.status,0,built.stderr+'\n'+messages.filter(m=>m.reason==='compiler-message').map(m=>m.message?.rendered??'').join(''));
const files=messages.filter(m=>m.reason==='compiler-artifact'&&m.target.name==='http_configured_payment_link_update'&&m.executable).map(m=>m.executable);assert.equal(files.length,1);
const timestamps=['10/15/2026','15-Oct-2026','2026/10/15','2026-1-2','1','49','50','2026-03-08T02:30:00','2026-11-01T01:30:00','2026-10-15T07:23:45','2026-02-31','2026-10-15T24:00','2026-01-01T00:00:00.00000000123456789Z','+275760-09-12T00:00','-271821-04-21T00:00','not-a-date','2026-13-01','2026-01-01T24:01','+275760-09-13T00:00:00.001Z'];
for(const TZ of ['UTC','America/Los_Angeles','America/New_York','Asia/Kathmandu']){
 const oracle=spawnSync(process.execPath,['--input-type=module','-e',`let input='';for await(const part of process.stdin)input+=part;console.log(JSON.stringify(JSON.parse(input).map(timestamp=>{const value=new Date(timestamp);return {timestamp,canonical:Number.isNaN(value.getTime())?null:value.toISOString(),status:Number.isNaN(value.getTime())?500:200}})));`],{env:{...env,TZ},input:JSON.stringify(timestamps),encoding:'utf8'});assert.equal(oracle.status,0,oracle.stderr);
 const tested=spawnSync(files[0],['--ignored','--nocapture'],{cwd:root,env:{...env,TZ,FORKLAUNCH_HMAC_DATE_CASES:oracle.stdout.trim()},stdio:'inherit',timeout:600000});assert.equal(tested.status,0,`${TZ}: ${tested.error?.message??''}`);
}
console.log(`PASS ${timestamps.length*4*3} additional actual-Node canonicalized HMAC HTTP/correlation probes across4zones, with existing request/MCP/SDK and zero-provider invalid controls; no runtime Node parser.`);
