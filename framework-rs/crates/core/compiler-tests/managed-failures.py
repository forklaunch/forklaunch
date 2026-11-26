#!/usr/bin/env python3
"""Emit real SDKs, then verify managed auth/timeout errors retain typed JSON contracts."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="managed-sdk-proof-") as directory:
    root = Path(directory)
    env = dict(os.environ, CARGO_INCREMENTAL="0", FORKLAUNCH_MANAGED_SDK_DIR=str(root))
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(repo / "framework-rs/Cargo.toml"), "-p", "forklaunch-core", "--features", "build,typed-schema", "--test", "managed_failures"], env=env, check=True)
    (root / "node_modules").symlink_to(Path(os.environ["FORKLAUNCH_SDK_NODE_MODULES"]), target_is_directory=True)
    (root / "package.json").write_text('{"type":"module"}')
    subprocess.run(["node", os.environ["FORKLAUNCH_TYPESCRIPT_COMPILER"], "--strict", "--target", "ES2022", "--module", "NodeNext", "--lib", "ES2022,DOM,DOM.Iterable", "client.ts"], cwd=root, check=True)
    (root / "oracle.mjs").write_text('''import assert from 'node:assert/strict';
import {createForkLaunchSdk} from './client.js';
const call=(status,body,media='application/json')=>createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(body),{status,headers:{'content-type':media}})}).managed.requests.pending();
for(const status of [401,403,408]) {
  const result=await call(status,{error:'fixed failure'});
  assert.equal(result.status,status);assert.equal(result.contentType,'application/json');assert.equal(result.response.error,'fixed failure');
  assert.deepEqual(result.headers,{});
  await assert.rejects(call(status,{error:123}));
  await assert.rejects(call(status,{error:'fixed'},'text/plain'));
}
await assert.rejects(call(418,{error:'undeclared'}));
console.log('PASS emitted TypeScript managed 401/403/408: typed JSON, optional application headers, invalid body/media/status rejected');
''')
    subprocess.run(["node", "oracle.mjs"], cwd=root, check=True)
    (root / "Cargo.toml").write_text('''[package]
name="managed-sdk-proof"
version="0.0.0"
edition="2024"
[workspace]
[lib]
path="client.rs"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="=1.0.151"
reqwest={version="0.13",default-features=false,features=["json","query","stream","multipart","rustls"]}
futures-util="0.3"
forklaunch-universal-sdk={path=''' + json.dumps(str(repo / "framework-rs/crates/universal-sdk")) + '''}
tokio={version="1",features=["macros","rt-multi-thread","net","io-util"]}
''')
    with (root / "client.rs").open("a") as handle:
        handle.write('''
#[cfg(test)] mod managed_proof {
use super::*;use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test] async fn managed_error_contracts_are_checked() {
for status in [401,403,408,418] { for variant in 0..3 {
 let valid=status!=418 && variant==0;
 let body=if variant==1 {"{\\"error\\":123}"}else{"{\\"error\\":\\"fixed failure\\"}"};
 let media=if variant==2 {"text/plain"}else{"application/json"};
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
 let server=tokio::spawn(async move {let(mut socket,_)=listener.accept().await.unwrap();let mut request=[0;8192];socket.read(&mut request).await.unwrap();socket.write_all(format!("HTTP/1.1 {status} Error\\r\\nContent-Type: {media}\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{body}",body.len()).as_bytes()).await.unwrap();});
 let client=ForkLaunchClient::new(format!("http://{address}"));
 let result=client.pending(PendingRequest::default()).await;assert_eq!(result.is_ok(),valid,"{status}/{variant}");
 if let Ok(response)=result {let message=match response {PendingResponses::Status401(value)=>value.error,PendingResponses::Status403(value)=>value.error,PendingResponses::Status408(value)=>value.error,_=>panic!("unexpected managed response")};assert_eq!(message,"fixed failure");}
 server.await.unwrap();
}}
}
}
''')
    subprocess.run(["cargo", "+stable", "test", "--manifest-path", str(root / "Cargo.toml")], env=env, check=True)
