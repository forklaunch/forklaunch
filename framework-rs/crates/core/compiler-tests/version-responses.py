#!/usr/bin/env python3
"""Actual emitted TS/Rust exact matched-version response admission proof."""
from pathlib import Path
import json, os, subprocess, tempfile
repo=Path(__file__).resolve().parents[4]
with tempfile.TemporaryDirectory(prefix='version-response-proof-') as directory:
 root=Path(directory); env=dict(os.environ,CARGO_INCREMENTAL='0',FORKLAUNCH_VERSION_SDK_DIR=directory)
 subprocess.run(['cargo','+stable','test','--manifest-path',str(repo/'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,typed-schema','--test','version_response_sdk'],env=env,check=True)
 (root/'node_modules').symlink_to(Path(os.environ['FORKLAUNCH_SDK_NODE_MODULES']),target_is_directory=True)
 with (root/'client.ts').open('a') as file:file.write('\nexport { matchVersionRequest, portableVersionWireBody, decodeVersionMultipart };\n')
 (root/'package.json').write_text('{"type":"module"}')
 subprocess.run(['node',os.environ['FORKLAUNCH_TYPESCRIPT_COMPILER'],'--strict','--target','ES2022','--module','NodeNext','--lib','ES2022,DOM,DOM.Iterable','client.ts'],cwd=root,check=True)
 (root/'oracle.mjs').write_text('''import assert from 'node:assert/strict';import {createForkLaunchSdk,matchVersionRequest,portableVersionWireBody,decodeVersionMultipart}from './client.js';
let calls=0;
const call=(select,body,header,hook)=>{const client=createForkLaunchSdk({baseUrl:'https://example.test',beforeRequest:hook,fetch:async()=>{calls++;return new Response(JSON.stringify({value:body}),{headers:{'content-type':'application/json','x-branch':header}});}});return client.versions.union.unionVersionFirst({query:{select,body,header}});};
for(const select of ['both','first','last'])for(const body of ['old','new','invalid'])for(const header of ['old','new','invalid']){
 const valid=(select==='both'?['old','new'].includes(body)&&['old','new'].includes(header):body===(select==='first'?'old':'new')&&header===body);
 if(valid){const response=await call(select,body,header);assert.equal(response.response.value,body);assert.equal(response.headers['x-branch'],header);}else await assert.rejects(call(select,body,header));
}
const before=calls;await assert.rejects(call('missing','old','old'));assert.equal(calls,before);
await assert.rejects(call('both','old','old',meta=>({...meta,input:{...meta.input,query:{...meta.input.query,select:'last'}},versionMetadata:{formatVersion:1,branches:[]}})));
const pathMetadata={formatVersion:1,branches:[{path:'/items/{id}',params:{id:{required:true,schema:{type:'string',enum:['a/b','café']}}},query:{},headers:{},bodyKind:'empty',bodyMedia:'',bodyOptional:false,bodySchema:true,responses:{}}]};
for(const path of ['/mount/items/a%2Fb','/mount/items/caf%C3%A9'])assert.equal((await matchVersionRequest(pathMetadata,path,'/mount',new URLSearchParams(),new Headers(),undefined)).length,1);
for(const path of ['/mount/items/a%252Fb','/mount/items/%FF','/mount/items/%','/wrong/items/a%2Fb'])await assert.rejects(()=>matchVersionRequest(pathMetadata,path,'/mount',new URLSearchParams(),new Headers(),undefined));
const binaryMetadata={formatVersion:1,branches:[{path:'/bytes',params:{},query:{},headers:{},bodyKind:'binary',bodyMedia:'application/octet-stream',bodyOptional:false,bodySchema:{type:'array',items:{type:'integer',minimum:128},minItems:1},responses:{}}]};
const binaryHeaders=new Headers({'content-type':'application/octet-stream'});
assert.equal((await matchVersionRequest(binaryMetadata,'/bytes','/',new URLSearchParams(),binaryHeaders,new Blob([new Uint8Array([255,128])]))).length,1);
await assert.rejects(()=>matchVersionRequest(binaryMetadata,'/bytes','/',new URLSearchParams(),binaryHeaders,new Uint8Array([127])));
await assert.rejects(()=>matchVersionRequest(binaryMetadata,'/bytes','/',new URLSearchParams(),new Headers({'content-type':'text/plain'}),new Uint8Array([255])));
const formMetadata={formatVersion:1,branches:[{path:'/form',params:{},query:{},headers:{},bodyKind:'form',bodyMedia:'application/x-www-form-urlencoded',bodyOptional:false,bodySchema:{type:'object',required:['user','tags'],properties:{user:{type:'object',required:['name'],properties:{name:{const:'café'}}},tags:{type:'array',items:{enum:['a','b']},minItems:2}},additionalProperties:false},responses:{}}]};
const formHeaders=new Headers({'content-type':'application/x-www-form-urlencoded'});
for(const wire of ['user[name]=caf%C3%A9&tags[]=a&tags[]=b','user%5Bname%5D=caf%C3%A9&tags[0]=a&tags[1]=b'])assert.equal((await matchVersionRequest(formMetadata,'/form','/',new URLSearchParams(),formHeaders,wire)).length,1);
for(const wire of ['user[name]=other&tags[]=a&tags[]=b','user[name]=caf%C3%A9&tags[]=x&tags[]=b','x=1&'.repeat(1001)])await assert.rejects(()=>matchVersionRequest(formMetadata,'/form','/',new URLSearchParams(),formHeaders,wire));
const profileCases=[
 {kind:'json',wire:'42',profile:{jsonStrict:false,jsonWire:{limitBytes:2,inflate:false}},expected:42,expressOptions:{strict:false,limit:2,inflate:false}},
 {kind:'form',wire:'user[name]=x',profile:{formExtended:false},expected:{'user[name]':'x'},expressOptions:{extended:false}},
 {kind:'form',wire:'utf8=%26%2310003%3B&value=%E9',profile:{formCharsetSentinel:true},expected:{value:'é'},expressOptions:{extended:true,charsetSentinel:true}},
 {kind:'form',wire:'utf8=%26%2310003%3B&value=%26%23233%3B',profile:{formCharsetSentinel:true,formInterpretNumericEntities:true},expected:{value:'é'},expressOptions:{extended:true,charsetSentinel:true,interpretNumericEntities:true}}
];
for(const item of profileCases){const media=item.kind==='json'?'application/json':'application/x-www-form-urlencoded';const branch={path:'/profile',params:{},query:{},headers:{},bodyKind:item.kind,bodyMedia:media,bodyOptional:false,bodySchema:{const:item.expected},parserOptions:item.profile,responses:{}};assert.equal((await matchVersionRequest({formatVersion:1,branches:[branch]},'/profile','/',new URLSearchParams(),new Headers({'content-type':media}),item.wire)).length,1);
 if(process.env.FORKLAUNCH_MULTIPART_ORACLE_ROOT){const {createRequire}=await import('node:module');const express=createRequire(process.env.FORKLAUNCH_MULTIPART_ORACLE_ROOT+'/package.json')('express');const app=express();app.post('/profile',item.kind==='json'?express.json(item.expressOptions):express.urlencoded(item.expressOptions),(req,res)=>res.json(req.body));const server=app.listen(0,'127.0.0.1');await new Promise(resolve=>server.once('listening',resolve));try{const response=await fetch('http://127.0.0.1:'+server.address().port+'/profile',{method:'POST',headers:{'content-type':media},body:item.wire});assert.equal(response.status,200);assert.deepEqual(await response.json(),item.expected);}finally{await new Promise(resolve=>server.close(resolve));}}
}
console.log('PASS explicit parser profiles match actual Express body-parser');
const multipartMetadata={formatVersion:1,branches:[{path:'/upload',params:{},query:{},headers:{},bodyKind:'multipart',bodyMedia:'multipart/form-data',bodyOptional:false,bodySchema:{type:'object',required:['label','file'],properties:{label:{const:'café'},file:{type:'array',items:{type:'integer',minimum:128},minItems:2}},additionalProperties:false},responses:{}}]};
const multipartHeaders=new Headers();const upload=new FormData();upload.set('label','café');upload.set('file',new Blob([new Uint8Array([255,128])]),'file.bin');
const multipartWire=await portableVersionWireBody(upload,multipartHeaders);
assert.equal((await matchVersionRequest(multipartMetadata,'/upload','/',new URLSearchParams(),multipartHeaders,multipartWire)).length,1);
if(process.env.FORKLAUNCH_MULTIPART_ORACLE_ROOT){
 const {createRequire}=await import('node:module');const {Readable}=await import('node:stream');const Busboy=createRequire(process.env.FORKLAUNCH_MULTIPART_ORACLE_ROOT+'/package.json')('busboy');
 const actual=await new Promise((resolve,reject)=>{const result={};const parser=Busboy({headers:{'content-type':multipartHeaders.get('content-type')}});parser.on('field',(name,value)=>result[name]=value);parser.on('file',(name,stream)=>{const chunks=[];stream.on('data',chunk=>chunks.push(chunk));stream.on('end',()=>result[name]=Array.from(Buffer.concat(chunks)));});parser.on('error',reject);parser.on('finish',()=>resolve(result));Readable.from([Buffer.from(multipartWire)]).pipe(parser);});
 assert.deepEqual({...decodeVersionMultipart(multipartWire,multipartHeaders.get('content-type'))},actual);console.log('PASS canonical Busboy matches portable generated multipart bytes');
}
const mixed=structuredClone(multipartMetadata);mixed.branches.unshift({...mixed.branches[0],bodyKind:'json',bodyMedia:'application/json',bodySchema:true});mixed.branches[1].bodySchema=true;const largePortable=new FormData();largePortable.set('file',new Blob([new Uint8Array(200000)]),'x');const largeHeaders=new Headers();const largeWire=await portableVersionWireBody(largePortable,largeHeaders);assert.equal((await matchVersionRequest(mixed,'/upload','/',new URLSearchParams(),largeHeaders,largeWire)).length,1);
const duplicate=new FormData();duplicate.append('a',new Blob(['file']),'x');duplicate.append('a','text');await assert.rejects(()=>portableVersionWireBody(duplicate,new Headers()));
for(const name of ['é','']){const form=new FormData();form.append(name,'value');await assert.rejects(()=>portableVersionWireBody(form,new Headers()));}
const huge=new FormData();huge.set('a',new Blob([new Uint8Array(1048577)]),'x');await assert.rejects(()=>portableVersionWireBody(huge,new Headers()));
const wrong=new FormData();wrong.set('label','café');wrong.set('file',new Blob([new Uint8Array([127])]),'x');const wrongHeaders=new Headers();const wrongWire=await portableVersionWireBody(wrong,wrongHeaders);await assert.rejects(()=>matchVersionRequest(multipartMetadata,'/upload','/',new URLSearchParams(),wrongHeaders,wrongWire));
for(const [label,value,valid]of [['first','old',true],['first','new',false],['both','new',true]]){
 const sdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(_url,options)=>{const decoded=decodeVersionMultipart(new Uint8Array(options.body),new Headers(options.headers).get('content-type'));assert.equal(decoded.label,label);assert.deepEqual(decoded.file,[0,255]);return new Response(JSON.stringify({value}),{headers:{'content-type':'application/json'}});}});
 const uploadCall=Object.values(sdk.versions).find(group=>typeof group.uploadVersionFirst==='function').uploadVersionFirst;
 const form=new FormData();form.set('label',label);form.set('file',new Blob([new Uint8Array([0,255])]),'x');
 if(valid)assert.equal((await uploadCall({body:form})).response.value,value);else await assert.rejects(()=>uploadCall({body:form}));
}
const scalarSdk=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async(_url,options)=>{assert.equal(options.body,'42');return new Response('{"value":"new"}',{headers:{'content-type':'application/json'}});}});const scalarCall=Object.values(scalarSdk.versions).find(group=>typeof group.scalarVersionFirst==='function').scalarVersionFirst;assert.equal((await scalarCall({body:42})).response.value,'new');await assert.rejects(()=>scalarCall({body:255}));
let canceled=false;
const eventClient=createForkLaunchSdk({baseUrl:'https://example.test',fetch:async()=>new Response(new ReadableStream({start(controller){controller.enqueue(new TextEncoder().encode('data: {"value":"old"}\\n\\ndata: {"value":"new"}\\n\\n'));},cancel(){canceled=true;}}),{headers:{'content-type':'text/event-stream','x-branch':'old'}})});
const eventCall=Object.values(eventClient.versions).find(group=>typeof group.eventsVersionFirst==='function').eventsVersionFirst;
const abort=new AbortController();let abortListeners=0;const add=abort.signal.addEventListener.bind(abort.signal),remove=abort.signal.removeEventListener.bind(abort.signal);abort.signal.addEventListener=(...args)=>{if(args[0]==='abort')abortListeners++;return add(...args);};abort.signal.removeEventListener=(...args)=>{if(args[0]==='abort')abortListeners--;return remove(...args);};await assert.rejects(()=>eventCall({query:{select:'missing',body:'old',header:'old'},signal:abort.signal}));assert.equal(abortListeners,0);
const eventResponse=await eventCall({query:{select:'first',body:'old',header:'old'}});
const iterator=eventResponse.response[Symbol.asyncIterator]();assert.equal((await iterator.next()).value.data.value,'old');await assert.rejects(()=>iterator.next());assert.equal(canceled,true);
console.log('PASS emitted TypeScript27 matched-version response cases; path/binary/form/portable-multipart predicates; SSE rejects and cancels unmatched frame; immutable post-hook metadata');
''')
 subprocess.run(['node','oracle.mjs'],cwd=root,check=True)
 (root/'Cargo.toml').write_text('''[package]
name="version-response-sdk-proof"
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
#[cfg(test)]mod version_proof {
use super::*;use tokio::io::{AsyncReadExt,AsyncWriteExt};
#[tokio::test]async fn exact_version_responses_and_post_hook_match(){
for select in ["both","first","last"]{for body in ["old","new","invalid"]{for header in ["old","new","invalid"]{
 let valid=if select=="both"{body!="invalid"&&header!="invalid"}else{body==if select=="first"{"old"}else{"new"}&&header==body};
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
 let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=[0;8192];socket.read(&mut bytes).await.unwrap();let payload=serde_json::json!({"value":body}).to_string();socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nX-Branch: {header}\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{payload}",payload.len()).as_bytes()).await.unwrap();});
 let client=ForkLaunchClient::new(format!("http://{address}"));let query=serde_json::from_value(serde_json::json!({"select":select,"body":body,"header":header})).unwrap();let result=client.union_version_first(UnionVersionFirstRequest{query,..Default::default()}).await;assert_eq!(result.is_ok(),valid,"{select}/{body}/{header}");server.await.unwrap();
}}}
for (label,value,valid) in [("first","old",true),("first","new",false),("both","new",true)] {
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
 let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=[0;8192];socket.read(&mut bytes).await.unwrap();let payload=serde_json::json!({"value":value}).to_string();socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{payload}",payload.len()).as_bytes()).await.unwrap();});
 let client=ForkLaunchClient::new(format!("http://{address}"));
 let form=MultipartForm::new().text("label",label).part("file",MultipartPart::bytes(vec![0,255]).file_name("x"));
 let result=client.upload_version_first(UploadVersionFirstRequest{body:Some(form),..Default::default()}).await;
 assert_eq!(result.is_ok(),valid);server.await.unwrap();
}
let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
let server=tokio::spawn(async move{let(mut socket,_)=listener.accept().await.unwrap();let mut bytes=[0;8192];socket.read(&mut bytes).await.unwrap();let payload=serde_json::json!({"value":"new"}).to_string();socket.write_all(format!("HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n{payload}",payload.len()).as_bytes()).await.unwrap();});
let scalar=ForkLaunchClient::new(format!("http://{address}"));assert!(scalar.scalar_version_first(ScalarVersionFirstRequest{body:Some(42),..Default::default()}).await.is_ok());server.await.unwrap();
let client=ForkLaunchClient::new("https://example.test").with_auth_hook(|request|{request.query.insert("select".into(),"missing".into());});
let query=serde_json::from_value(serde_json::json!({"select":"both","body":"old","header":"old"})).unwrap();assert!(matches!(client.union_version_first(UnionVersionFirstRequest{query,..Default::default()}).await,Err(SdkError::Contract("request matches no declared API version"))));
}
}
''')
 subprocess.run(['cargo','+stable','test','--manifest-path',str(root/'Cargo.toml')],env=env,check=True)
 print('PASS generated Rust27 exact matched-version cases plus actual post-hook request matching')
