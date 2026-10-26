#!/usr/bin/env python3
"""Run under the shared Cargo fixture lock: native parsing, Zod, and emitted SDKs."""
from pathlib import Path
import json, os, subprocess, tempfile
repo = Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix="normalization-contract-parity-") as directory:
    root=Path(directory)
    env=dict(os.environ,CARGO_INCREMENTAL="0",FORKLAUNCH_NORMALIZATION_FIXTURE=str(root/'proof.json'),FORKLAUNCH_NORMALIZATION_SDK_DIR=str(root))
    subprocess.run(['cargo','+stable','test','--manifest-path',str(repo/'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,typed-schema','--test','normalization_schema'],env=env,check=True)
    modules=Path(os.environ['FORKLAUNCH_SDK_NODE_MODULES']);compiler=Path(os.environ['FORKLAUNCH_TYPESCRIPT_COMPILER'])
    (root/'node_modules').symlink_to(modules,target_is_directory=True);(root/'package.json').write_text('{"type":"module"}')
    subprocess.run(['node',str(compiler),'--strict','--target','ES2022','--module','NodeNext','--lib','ES2022,DOM,DOM.Iterable','client.ts'],cwd=root,check=True)
    (root/'oracle.mjs').write_text('''import {z} from 'zod'; import assert from 'node:assert/strict'; import {readFileSync} from 'node:fs';
import {createForkLaunchSdk} from './client.js';
const scalarLength=value=>[...value].length;
const title=z.string().refine(value=>scalarLength(value)<=12).transform(value=>value.replace(/^[\\u0009-\\u000d ]+|[\\u0009-\\u000d ]+$/g,''))
 .refine(value=>scalarLength(value)>=1&&scalarLength(value)<=4);
const schema=z.object({title}).strict();
const proof=JSON.parse(readFileSync(new URL('./proof.json',import.meta.url),'utf8'));
for(const {input,accepted,normalized,outputAccepted} of proof.cases){
const parsed=schema.safeParse(input);
assert.equal(parsed.success,accepted,'Zod/native normalization mismatch');
if(parsed.success)assert.deepEqual(parsed.data,normalized,'normalized handler value mismatch');
const sdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(input),{status:200,headers:{'content-type':'application/json'}})});
let actual;try{await sdk.normalization.normalize.normalize({body:{title:'ok'}});actual=true;}catch{actual=false;}
assert.equal(actual,outputAccepted,'generated TypeScript SDK normalized response mismatch');
}
console.log(`PASS Zod and emitted TypeScript SDK: ${proof.cases.length} directional normalization cases`);
''')
    subprocess.run(['node','oracle.mjs'],cwd=root,check=True)
    (root/'Cargo.toml').write_text('''[package]
name="normalization-sdk-runtime-proof"
version="0.0.0"
edition="2024"
[workspace]
[lib]
path="client.rs"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="=1.0.151"
reqwest={version="0.13",default-features=false,features=["json","query","stream","multipart","rustls"]}
forklaunch-universal-sdk={path='''+json.dumps(str(repo/'framework-rs/crates/universal-sdk'))+'''}
tokio={version="1",features=["macros","rt-multi-thread","net","io-util"]}
''')
    with (root/'client.rs').open('a') as handle:
        handle.write('''\n#[cfg(test)] mod normalization_remote_responses {use super::*;use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test]async fn unnormalized_remote_values_fail_closed(){
let proof:serde_json::Value=serde_json::from_str(include_str!("proof.json")).unwrap();
for case in proof["cases"].as_array().unwrap(){
let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let body=case["input"].to_string();
let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut request=[0;8192];socket.read(&mut request).await.unwrap();socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{}",body.len(),body).as_bytes()).await.unwrap();});
let result=ForkLaunchClient::new(format!("http://{address}")).normalize(NormalizeRequest::default()).await;
assert_eq!(result.is_ok(),case["outputAccepted"].as_bool().unwrap(),"remote normalized schema mismatch");server.await.unwrap();
}}}\n''')
    subprocess.run(['cargo','+stable','test','--manifest-path',str(root/'Cargo.toml')],env=env,check=True)
