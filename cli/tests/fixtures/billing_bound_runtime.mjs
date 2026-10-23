// Shared body of the rust_billing_bound_*_runtime.mjs scripts: build one
// forklaunch-billing-stripe integration test with the bound-checkout feature,
// then run its ignored live HTTP/MCP/SDK cases. Each script names its test.
// Release Cargo's build lock before the live fixture compiles its Rust SDK client.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export function runBoundRuntime(test, label) {
  const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
  const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_TARGET_DIR:path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target'))};
  const built=spawnSync('cargo',['test','--manifest-path',path.join(root,'blueprint-rs/Cargo.toml'),'-p','forklaunch-billing-stripe','--features','bound-checkout','--test',test,'--no-run','--message-format=json'],{cwd:root,env,encoding:'utf8',timeout:600000,maxBuffer:32*1024*1024});
  const diagnostics=built.stdout.split('\n').flatMap(line=>{try{const message=JSON.parse(line);return message.reason==='compiler-message'&&message.message?.rendered?[message.message.rendered]:[];}catch{return [];}}).join('\n');
  assert.equal(built.status,0,built.stderr+'\n'+diagnostics);
  const executables=built.stdout.split('\n').filter(Boolean).map(JSON.parse).filter(message=>message.reason==='compiler-artifact'&&message.target.name===test&&message.executable).map(message=>message.executable);
  assert.equal(executables.length,1,`one bound ${label} test executable`);
  const tested=spawnSync(executables[0],['--ignored','--nocapture'],{cwd:root,env,stdio:'inherit',timeout:600000});
  assert.equal(tested.status,0,tested.error?.message);
}
