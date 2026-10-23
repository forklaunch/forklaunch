#!/usr/bin/env python3
"""Actual generated HTTP/SSE clients: native chrono and JavaScript Date codecs."""
from pathlib import Path
import json,os,subprocess,tempfile
repo=Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix='date-codec-proof-') as directory:
 root=Path(directory);env=dict(os.environ,CARGO_INCREMENTAL='0',FORKLAUNCH_DATE_SDK_DIR=str(root))
 subprocess.run(['cargo','+stable','test','--manifest-path',str(repo/'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,typed-schema','--test','datetime_schema'],env=env,check=True)
 (root/'node_modules').symlink_to(Path(os.environ['FORKLAUNCH_SDK_NODE_MODULES']),target_is_directory=True)
 (root/'package.json').write_text('{"type":"module"}')
 subprocess.run(['node',os.environ['FORKLAUNCH_TYPESCRIPT_COMPILER'],'--strict','--target','ES2022','--module','NodeNext','--lib','ES2022,DOM,DOM.Iterable','client.ts'],cwd=root,check=True)
 (root/'oracle.mjs').write_text('''import assert from 'node:assert/strict';import {z} from 'zod';import {createForkLaunchSdk} from './client.js';
const stamp='2024-02-29T12:34:56.123Z';const payload={primary:{created_at:stamp},entries:[{created_at:stamp}],ordinary:stamp};
let requests=0;
const sdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(url,init)=>{requests++;const body=JSON.parse(init.body);assert.deepEqual(body,payload);return new Response(JSON.stringify(payload),{headers:{'content-type':'application/json'}});}});
const native={primary:{created_at:new Date(stamp)},entries:[{created_at:new Date(stamp)}],ordinary:stamp};
const response=await sdk.dates.dates.apply({body:native});assert(response.response.primary.created_at instanceof Date);assert(response.response.entries[0].created_at instanceof Date);assert.equal(response.response.primary.created_at.toISOString(),stamp);assert.equal(response.response.ordinary,stamp);
for(const value of [stamp,new Date(NaN),new Date('+010000-01-01T00:00:00.000Z')])await assert.rejects(sdk.dates.dates.apply({body:{...native,primary:{created_at:value}}}));assert.equal(requests,1);
for(const value of ['2023-02-29T00:00:00.000Z','2024-02-29T12:34:60.000Z','2024-02-29T12:34:56.1234Z','2024-02-29T12:34:56.000+00:00','2024-02-29T12:34:56Z']){
const bad={...payload,primary:{created_at:value}};
assert.equal(z.string().datetime({precision:3}).safeParse(value).success,false);
const remote=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(JSON.stringify(bad),{headers:{'content-type':'application/json'}})});await assert.rejects(remote.dates.dates.apply({body:native}));
let closed=false;const stream=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(new ReadableStream({start(c){c.enqueue(new TextEncoder().encode('data: '+JSON.stringify(bad)+'\\n\\n'));},cancel(){closed=true;}}),{headers:{'content-type':'text/event-stream'}})});
const events=(await stream.dates.dates.watch()).response;await assert.rejects(events.next());assert(closed);
}
const streamed=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response('data: '+JSON.stringify(payload)+'\\n\\n',{headers:{'content-type':'text/event-stream'}})});const event=await(await streamed.dates.dates.watch()).response.next();assert(event.value.data.primary.created_at instanceof Date);assert(event.value.data.entries[0].created_at instanceof Date);
const shapeWire={dates:[stamp,null],pair:[stamp,stamp],alias:stamp,nullable:null};
const shapeInput={dates:[new Date(stamp),null],pair:[new Date(stamp),new Date(stamp)],alias:new Date(stamp),nullable:null};
let shapesSent=0;
const shapes=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(url,init)=>{shapesSent++;assert.deepEqual(JSON.parse(init.body),shapeWire);return new Response(JSON.stringify(shapeWire),{headers:{'content-type':'application/json'}});}});
const shaped=(await shapes.dates.dates.shapes({body:shapeInput})).response;
assert(shaped.alias instanceof Date);assert(shaped.dates[0] instanceof Date);assert.equal(shaped.dates[1],null);assert(shaped.pair[1] instanceof Date);assert.equal(shaped.nullable,null);assert(!Object.hasOwn(shaped,'supplied'));
for(const change of [{supplied:null},{pair:[new Date(stamp)]},{dates:[new Date(NaN)]},{alias:stamp}])await assert.rejects(shapes.dates.dates.shapes({body:{...shapeInput,...change}}));assert.equal(shapesSent,1);
console.log('PASS generated TypeScript HTTP and SSE: recursive Dates, request encoding, invalid dates, precision, ordinary strings and cancellation');
''')
 subprocess.run(['node','oracle.mjs'],cwd=root,check=True)
 (root/'Cargo.toml').write_text('''[package]
name="datetime-sdk-runtime-proof"
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
tokio-tungstenite="0.30"
forklaunch-ws={path='''+json.dumps(str(repo/'framework-rs/crates/ws'))+''',features=["transport","json-schema"]}
forklaunch-universal-sdk={path='''+json.dumps(str(repo/'framework-rs/crates/universal-sdk'))+'''}
tokio={version="1",features=["macros","rt-multi-thread","net","io-util"]}
''')
 with (root/'client.rs').open('a') as handle:
  handle.write('''
#[path="socket.rs"]mod dates_socket;
#[cfg(test)] mod dates_remote {use super::*;use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test]async fn generated_socket_dates_are_checked_in_both_directions(){
use futures_util::{SinkExt,StreamExt};
let stamp="2024-02-29T12:34:56.123Z";
let wire=serde_json::json!({"dates":[stamp,null],"pair":[stamp,stamp],"alias":stamp,"nullable":null});
let mut invalid:dates_socket::SocketOutgoing=serde_json::from_value(wire.clone()).unwrap();
invalid.alias=runtime::datetime::DateTime::parse_from_rfc3339("2024-02-29T12:34:56.123456Z").unwrap().with_timezone(&runtime::datetime::Utc);
assert!(dates_socket::ForkLaunchClient::schemas().unwrap().outgoing.encode(&invalid).is_err());
for valid in [true,false]{
let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let expected=wire.clone();
let server=tokio::spawn(async move {let(socket,_)=listener.accept().await.unwrap();let mut socket=tokio_tungstenite::accept_async(socket).await.unwrap();let request=socket.next().await.unwrap().unwrap();assert_eq!(serde_json::from_slice::<serde_json::Value>(&request.into_data()).unwrap(),expected);let mut response=expected;if !valid{response["alias"]=serde_json::json!("2024-02-29T12:34:56.1234Z");}socket.send(tokio_tungstenite::tungstenite::Message::text(response.to_string())).await.unwrap();});
let(mut client,_)=dates_socket::ForkLaunchClient::connect(format!("ws://{address}/dates"),forklaunch_ws::transport::SocketOptions::default()).await.unwrap();let message=serde_json::from_value(wire.clone()).unwrap();client.send(&message).await.unwrap();let response=client.recv().await;assert_eq!(response.is_ok(),valid);if let Ok(Some(forklaunch_ws::transport::SocketEvent::Message(value)))=response {assert_eq!(value.alias.timestamp_millis(),1709210096123);assert!(value.nullable.unwrap().is_none());}server.await.unwrap();
}
}
#[test]fn generated_date_shapes_preserve_null_and_absence(){
let stamp="2024-02-29T12:34:56.123Z";
let wire=serde_json::json!({"dates":[stamp,null],"pair":[stamp,stamp],"alias":stamp,"nullable":null});
let mut value:ShapesRequestBody=serde_json::from_value(wire.clone()).unwrap();
assert!(value.nullable.as_ref().unwrap().is_none());assert!(value.supplied.is_none());assert_eq!(value.dates[0].unwrap().timestamp_millis(),1709210096123);assert_eq!(serde_json::to_value(&value).unwrap(),wire);
value.alias=runtime::datetime::DateTime::parse_from_rfc3339("2024-02-29T12:34:56.123456Z").unwrap().with_timezone(&runtime::datetime::Utc);
assert!(serde_json::to_value(&value).is_err());
}
#[tokio::test] async fn native_dates_cross_http_and_streams(){
let stamp="2024-02-29T12:34:56.123Z";
for (value,valid) in [(stamp,true),("2023-02-29T00:00:00.000Z",false),("2024-02-29T12:34:60.000Z",false),("2024-02-29T12:34:56.1234Z",false),("2024-02-29T12:34:56Z",false)] {
for streaming in [false,true]{
let payload=serde_json::json!({"primary":{"created_at":value},"entries":[{"created_at":value}],"ordinary":stamp});
let body=if streaming{format!("data: {}\\n\\n",payload)}else{payload.to_string()};
let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut request=[0;8192];socket.read(&mut request).await.unwrap();let media=if streaming{"text/event-stream"}else{"application/json"};socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: {media}\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{}",body.len(),body).as_bytes()).await.unwrap();});
let client=ForkLaunchClient::new(format!("http://{address}"));
if streaming {let response=client.watch(WatchRequest::default()).await.unwrap(); if let WatchResponses::Status200(mut events)=response{let event=events.next().await;assert_eq!(event.is_ok(),valid);if let Ok(Some(event))=event{assert_eq!(event.data.primary.created_at.timestamp_millis(),1709210096123);}}else{panic!("expected event stream")}}
else {let body=serde_json::from_value(serde_json::json!({"primary":{"created_at":stamp},"entries":[{"created_at":stamp}],"ordinary":stamp})).unwrap();let response=client.apply(ApplyRequest{body:Some(body),..Default::default()}).await;assert_eq!(response.is_ok(),valid);if let Ok(ApplyResponses::Status200(value))=response {assert_eq!(value.primary.created_at.timestamp_millis(),1709210096123);}}
server.await.unwrap();
}}
}}
''')
 subprocess.run(['cargo','+stable','test','--manifest-path',str(root/'Cargo.toml')],env=env,check=True)
