// Compile checked portable artifacts, then exercise both clients against real ws frames.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, writeFile, symlink, rm } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const framework = path.resolve(root, process.env.FORKLAUNCH_TEST_FRAMEWORK_ROOT ?? 'framework-rs');
const deps = createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES, 'package.json'));
const { WebSocketServer } = deps('ws');
const temporary = await mkdtemp(path.join(tmpdir(), 'fl-ws-binary-'));
const env = { ...process.env, CARGO_INCREMENTAL:'0', CARGO_TARGET_DIR:path.resolve(root, process.env.CARGO_TARGET_DIR ?? 'framework-rs/target') };
const imported = process.env.FORKLAUNCH_TEST_BINARY_ASYNCAPI === '1';
const binary = {binary:true, shape:{type:'string',format:'binary'}};
const sockets = new Set();
let server, child, client;
try {
  await mkdir(path.join(temporary, 'src'));
  await symlink(path.join(path.dirname(deps.resolve('ws/package.json')), '..'), path.join(temporary, 'node_modules'), 'dir');
  await writeFile(path.join(temporary,'package.json'), JSON.stringify({type:'module'}));
  const artifact = {formatVersion:1,title:'Binary',version:'1',sockets:{bytes:{path:'/bytes',clientMessages:{request:binary},serverMessages:{reply:binary},ping:binary,pong:binary}}};
  const input = path.join(temporary,'contract.json');
  const document = {
    asyncapi:'3.0.0',info:{title:'Binary import',version:'1'},defaultContentType:'application/octet-stream',
    servers:{local:{host:'localhost',protocol:'ws'}},
    channels:{bytes:{address:'/bytes',messages:{request:{$ref:'#/components/messages/Bytes'},reply:{$ref:'#/components/messages/Bytes'}}}},
    operations:{
      receive:{action:'receive',channel:{$ref:'#/channels/bytes'},messages:[{$ref:'#/channels/bytes/messages/request'}]},
      send:{action:'send',channel:{$ref:'#/channels/bytes'},messages:[{$ref:'#/channels/bytes/messages/reply'}]}
    },components:{messages:{Bytes:{payload:{$ref:'#/components/schemas/Bytes'}}},schemas:{Bytes:binary.shape}}
  };
  await writeFile(input,JSON.stringify(imported ? document : artifact));
  const generated = JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(framework,'Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','typescript_socket_artifacts','--',input,'all', imported ? 'asyncapi' : 'socket-contract'],{cwd:root,env,timeout:300000,maxBuffer:8*1024*1024,encoding:'utf8'}));
  if (imported) assert.deepEqual(generated.asyncapi, document, 'external document must be preserved');
  for (const channel of Object.values(imported ? {} : generated.asyncapi.channels)) {
    for (const message of Object.values(channel.messages)) assert.equal(message.contentType,'application/octet-stream');
  }
  await writeFile(path.join(temporary,'src/generated.ts'), generated.typescript.socket_bytes+'\nexport { SocketCodec };\n');
  await writeFile(path.join(temporary,'tsconfig.json'), JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true,types:['node'],outDir:'dist',rootDir:'src',skipLibCheck:false},include:['src/*.ts']}));
  execFileSync(process.env.TSC_BIN ?? deps.resolve('typescript/bin/tsc'),['--project',path.join(temporary,'tsconfig.json')],{stdio:'inherit',timeout:60000});
  const {SocketClient,SocketCodec} = await import(pathToFileURL(path.join(temporary,'dist/generated.js')));
  const codec = new SocketCodec(binary.shape, 4, true);
  for(const bytes of [Buffer.alloc(0),Buffer.from([0,255,128,34]),Buffer.from('null'),Buffer.from('123')]) {
    assert.deepEqual(Buffer.from(codec.decode(bytes)),bytes);
    assert.deepEqual(codec.encode(Uint8Array.from(bytes)),bytes);
  }
  assert.throws(()=>codec.encode('text'),/Uint8Array/);
  assert.throws(()=>codec.encode(new Uint8Array(5)),/byte limit/);
  assert.throws(()=>codec.decode(Buffer.alloc(5)),/byte limit/);
  const original = Buffer.from([255]); const decoded = codec.decode(original); original[0]=0;
  assert.equal(decoded[0],255,'decoded data must own its bytes');
  await writeFile(path.join(temporary,'Cargo.toml'), `[package]\nname="forklaunch-binary-socket-fixture"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\nforklaunch-ws={path=${JSON.stringify(path.join(framework,'crates/ws'))},features=["transport","json-schema"]}\nserde={version="1",features=["derive"]}\nserde_json="1"\ntokio={version="1",features=["net","macros","rt-multi-thread"]}\ntokio-tungstenite="0.30"\n`);
  await writeFile(path.join(temporary,'src/lib.rs'),generated.rust.socket_bytes);
  await writeFile(path.join(temporary,'src/main.rs'), `use forklaunch_binary_socket_fixture::*;
use forklaunch_ws::transport::{SocketEvent,SocketOptions};
#[tokio::main] async fn main() {
let options=SocketOptions {max_message_bytes:256,..SocketOptions::default()};
let (mut client, _)=SocketClient::connect(std::env::args().nth(1).unwrap(),options).await.unwrap();
for bytes in [vec![],vec![0,255,128,34],b"null".to_vec(),b"123".to_vec(),(0..=255).collect()] {
client.send(&bytes).await.unwrap();
match client.recv().await.unwrap().unwrap(){SocketEvent::Message(value)=>assert_eq!(value,bytes),_=>panic!("expected message")}
}
assert!(client.send(&vec![0;257]).await.is_err());
${imported ? '' : `client.ping(Some(&vec![0,255,128])).await.unwrap();
match client.recv().await.unwrap().unwrap(){SocketEvent::Pong(value)=>assert_eq!(value,vec![0,255,128]),_=>panic!("expected pong")}
assert!(client.ping(Some(&vec![0;126])).await.is_err());`}
client.close(1000,None).await.unwrap();
println!("PASS generated Rust binary messages and limits${imported ? '' : ', including control frames'}");
}`);
  execFileSync('cargo',['build','--quiet','--manifest-path',path.join(temporary,'Cargo.toml')],{cwd:root,env,stdio:'inherit',timeout:300000});
  server = new WebSocketServer({host:'127.0.0.1',port:0});
  await once(server,'listening');
  let messages=0;
  server.on('connection',socket=>{
    sockets.add(socket); socket.on('close',()=>sockets.delete(socket));
    socket.on('message',(bytes,isBinary)=>{assert.equal(isBinary,true);messages++;socket.send(bytes,{binary:true});});
  });
  const url=`ws://127.0.0.1:${server.address().port}/bytes`;
  client=await SocketClient.connect(url,{maxMessageBytes:256});
  for(const bytes of [Buffer.alloc(0),Buffer.from([0,255,128,34]),Buffer.from('null'),Buffer.from('123'),Buffer.from(Array.from({length:256},(_,i)=>i))]) {
    await client.send(Uint8Array.from(bytes));
    const event=await client.recv({timeoutMs:5000}); assert.equal(event.type,'message');
    assert.deepEqual(Buffer.from(event.value),bytes);
  }
  await assert.rejects(client.send(new Uint8Array(257)),/byte limit/);
  if (!imported) {
  await client.ping(new Uint8Array([0,255,128]));
  const pong=await client.recv({timeoutMs:5000}); assert.equal(pong.type,'pong'); assert.deepEqual([...pong.value],[0,255,128]);
  await assert.rejects(client.ping(new Uint8Array(126)),/byte limit/);
  }
  await client.close(1000);
  child=spawn(path.join(env.CARGO_TARGET_DIR,'debug/forklaunch-binary-socket-fixture'),[url],{stdio:'inherit'});
  const timeout=setTimeout(()=>child.kill('SIGKILL'),15000);
  try { const [code]=await once(child,'exit'); assert.equal(code,0); } finally {clearTimeout(timeout);}
  assert.equal(messages,10);
  console.log(imported ? 'PASS external AsyncAPI binary import, both compiled SDKs and 10 live byte roundtrips' : 'PASS checked binary artifact, AsyncAPI metadata, compiled Rust/TypeScript SDKs, raw frames, invalid UTF-8, JSON-looking bytes, control frames and bounds');
} finally {
  if(child?.exitCode===null) child.kill('SIGKILL');
  for(const socket of sockets)socket.terminate();
  if(server) await new Promise(resolve=>server.close(resolve));
  await rm(temporary,{recursive:true,force:true});
}
