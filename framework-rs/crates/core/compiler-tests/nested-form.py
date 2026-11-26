#!/usr/bin/env python3
"""Actual emitted TS/Rust typed form requests and shared native decoding proof."""
from pathlib import Path
import json, os, subprocess, tempfile
repo=Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix='nested-form-proof-') as directory:
 root=Path(directory); env=dict(os.environ,CARGO_INCREMENTAL='0',FORKLAUNCH_FORM_SDK_DIR=directory)
 subprocess.run(['cargo','+stable','test','--manifest-path',str(repo/'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,typed-schema','--test','form_schema'],env=env,check=True)
 (root/'node_modules').symlink_to(Path(os.environ['FORKLAUNCH_SDK_NODE_MODULES']),target_is_directory=True)
 (root/'package.json').write_text('{"type":"module"}')
 subprocess.run(['node',os.environ['FORKLAUNCH_TYPESCRIPT_COMPILER'],'--strict','--target','ES2022','--module','NodeNext','--lib','ES2022,DOM,DOM.Iterable','client.ts'],cwd=root,check=True)
 (root/'oracle.mjs').write_text('''import assert from 'node:assert/strict';import fs from 'node:fs';import {createForkLaunchSdk} from './client.js';
const value={profile:{name:'007 💙',ages:[1,65535],enabled:true},labels:['a+b','c&d']};let calls=0;
const api=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(url,init)=>{calls++;assert.ok(init.body instanceof URLSearchParams);assert.equal(init.body.get('profile[ages][1]'),'65535');assert.equal(init.body.get('profile[name]'),value.profile.name);fs.writeFileSync('typescript-form.txt',init.body.toString());return new Response(JSON.stringify(value),{headers:{'content-type':'application/json'}});}}).forms.forms;
assert.deepEqual((await api.apply({body:value})).response,value);
for(const body of [{profile:null},{profile:{'0':'ambiguous'}},{profile:[]},{profile:{}},{'a[b]':'x'},JSON.parse('{"__proto__":"x"}'),{x:'\\ud800'},{x:[,1]},{x:'x'.repeat(102401)},new Date()])await assert.rejects(api.apply({body}));assert.equal(calls,1);
console.log('PASS generated TypeScript nested indexed form, Unicode and lossless encoding rejection');
''')
 subprocess.run(['node','oracle.mjs'],cwd=root,check=True)
 (root/'Cargo.toml').write_text('''[package]
name="nested-form-sdk-proof"
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
forklaunch-universal-sdk={path='''+json.dumps(str(repo/'framework-rs/crates/universal-sdk'))+'''}
forklaunch-core={path='''+json.dumps(str(repo/'framework-rs/crates/core'))+''',features=["http"]}
tokio={version="1",features=["macros","rt-multi-thread","net","io-util"]}
''')
 with (root/'client.rs').open('a') as file:file.write('''
#[cfg(test)]mod form_proof {
use super::*;use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test]async fn actual_typed_request_and_typescript_wire_decode(){
 let expected=serde_json::json!({"profile":{"name":"007 💙","ages":[1,65535],"enabled":true},"labels":["a+b","c&d"]});
 let ts:ApplyRequestBody=forklaunch_core::http::decode_urlencoded(include_bytes!("typescript-form.txt"),"application/x-www-form-urlencoded").unwrap();assert_eq!(serde_json::to_value(ts).unwrap(),expected);
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let response=expected.clone();
 let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=Vec::new();let mut chunk=[0;4096];loop {let n=socket.read(&mut chunk).await.unwrap();assert!(n>0);bytes.extend_from_slice(&chunk[..n]);if let Some(offset)=bytes.windows(4).position(|x|x==b"\\r\\n\\r\\n"){let headers=String::from_utf8_lossy(&bytes[..offset]);let length:usize=headers.lines().find_map(|line|line.to_ascii_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse().unwrap())).unwrap();if bytes.len()>=offset+4+length {let body:ApplyRequestBody=forklaunch_core::http::decode_urlencoded(&bytes[offset+4..offset+4+length],"application/x-www-form-urlencoded").unwrap();assert_eq!(serde_json::to_value(body).unwrap(),response);break;}}}
 let body=serde_json::to_string(&response).unwrap();socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{body}",body.len()).as_bytes()).await.unwrap();});
 let client=ForkLaunchClient::new(format!("http://{address}"));let result=client.apply(ApplyRequest{body:Some(serde_json::from_value(expected.clone()).unwrap()),..Default::default()}).await.unwrap();match result{ApplyResponses::Status200(value)=>assert_eq!(serde_json::to_value(value).unwrap(),expected),_=>panic!("wrong status")};server.await.unwrap();
}
}
''')
 subprocess.run(['cargo','+stable','test','--manifest-path',str(root/'Cargo.toml')],env=env,check=True)
 print('PASS emitted Rust typed form over HTTP, native DTO decoder, TypeScript wire interoperability')
