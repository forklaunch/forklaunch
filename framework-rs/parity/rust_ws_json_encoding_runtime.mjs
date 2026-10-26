// Imported JSON strings/null retain JSON wire encoding in both generated clients.
import assert from 'node:assert/strict';
import {execFileSync, spawn} from 'node:child_process';
import {once} from 'node:events';
import {mkdtemp, mkdir, writeFile, symlink, rm} from 'node:fs/promises';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const framework=path.join(root,'framework-rs');
const deps=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {WebSocketServer}=deps('ws');
const temporary=await mkdtemp(path.join(tmpdir(),'fl-ws-json-encoding-'));
const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_TARGET_DIR:path.resolve(root,process.env.CARGO_TARGET_DIR??'framework-rs/target')};
const values=['', '123', 'null', 'false', '{"value":1}', '水 🦀', 'first\nsecond', null];
const document={asyncapi:'3.0.0',info:{title:'JSON encoding',version:'1'},defaultContentType:'application/json',servers:{local:{host:'localhost',protocol:'ws'}},channels:{json:{address:'/json',messages:{value:{payload:{type:['string','null']}}}}},operations:{}};
for(const action of ['receive','send']) document.operations[action]={action,channel:{$ref:'#/channels/json'},messages:[{$ref:'#/channels/json/messages/value'}]};
let server,child;
const sockets=new Set();
try {
 await mkdir(path.join(temporary,'src'));
 await symlink(path.join(path.dirname(deps.resolve('ws/package.json')),'..'),path.join(temporary,'node_modules'),'dir');
 await writeFile(path.join(temporary,'package.json'),JSON.stringify({type:'module'}));
 const input=path.join(temporary,'asyncapi.json');
 await writeFile(input,JSON.stringify(document));
 const generated=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(framework,'Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','typescript_socket_artifacts','--',input,'all','asyncapi'],{cwd:root,env,timeout:300000,maxBuffer:16*1024*1024,encoding:'utf8'}));
 assert.deepEqual(generated.asyncapi,document);
 const modules=Object.keys(generated.rust);
 for(const name of modules) {
  await writeFile(path.join(temporary,`src/${name}.ts`),generated.typescript[name]+'\nexport { SocketCodec };\n');
  await writeFile(path.join(temporary,`src/${name}.rs`),generated.rust[name]);
 }
 await writeFile(path.join(temporary,'src/lib.rs'),modules.map(name=>`pub mod ${name};`).join('\n'));
 await writeFile(path.join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true,types:['node'],outDir:'dist',rootDir:'src'},include:['src/*.ts']}));
 execFileSync(process.env.TSC_BIN,['--project',path.join(temporary,'tsconfig.json')],{stdio:'inherit',timeout:60000});
 await writeFile(path.join(temporary,'Cargo.toml'),`[package]\nname="forklaunch-json-encoding-fixture"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\nforklaunch-ws={path=${JSON.stringify(path.join(framework,'crates/ws'))},features=["transport","json-schema"]}\nserde={version="1",features=["derive"]}\nserde_json="1"\ntokio={version="1",features=["net","macros","rt-multi-thread"]}\ntokio-tungstenite="0.30"\n`);
 await writeFile(path.join(temporary,'src/main.rs'),`use forklaunch_ws::transport::{SocketEvent,SocketOptions};
#[tokio::main] async fn main() {
let base=std::env::args().nth(1).unwrap();
${modules.map(name=>`{
use forklaunch_json_encoding_fixture::${name}::SocketClient;
let (mut client,_)=SocketClient::connect(format!("{}/{}",base,"${name.replace('socket_','')}"),SocketOptions::default()).await.unwrap();
for value in serde_json::from_str::<Vec<serde_json::Value>>(r#"${JSON.stringify(values)}"#).unwrap() {
let message=serde_json::from_value(value.clone()).unwrap();client.send(&message).await.unwrap();
match client.recv().await.unwrap().unwrap(){SocketEvent::Message(received)=>assert_eq!(serde_json::to_value(received).unwrap(),value),_=>panic!("expected message")}
}
client.send(&Some("badjson".to_owned())).await.unwrap();
assert!(client.recv().await.is_err());
drop(client);
}`).join('\n')}
println!("PASS Rust JSON strings, null and malformed peer rejection");
}`);
 execFileSync('cargo',['build','--quiet','--manifest-path',path.join(temporary,'Cargo.toml')],{cwd:root,env,stdio:'inherit',timeout:300000});
 server=new WebSocketServer({host:'127.0.0.1',port:0});await once(server,'listening');
 let messages=0;
 server.on('connection',socket=>{sockets.add(socket);socket.on('close',()=>sockets.delete(socket));socket.on('message',bytes=>{const value=JSON.parse(bytes.toString());
if(value==='badjson'){socket.send('unquoted text');return;}
assert.deepEqual(value,values[messages % values.length]);assert.equal(bytes.toString(),JSON.stringify(value));messages++;socket.send(bytes,{binary:false});});});
 const base=`ws://127.0.0.1:${server.address().port}`;
 for(const name of modules) {
  const {SocketClient,SocketCodec}=await import(pathToFileURL(path.join(temporary,`dist/${name}.js`)));
  const codec=new SocketCodec({type:['string','null']},4,false,true);
  assert.equal(codec.encode('ab').toString(),'"ab"');assert.equal(codec.decode(Buffer.from('null')),null);
  assert.throws(()=>codec.encode('abc'),/byte limit/);assert.throws(()=>codec.decode(Buffer.from('"abc"')),/byte limit/);
  for(const bytes of [Buffer.from('raw'),Buffer.from([34,255,34]),Buffer.alloc(0)]) assert.throws(()=>codec.decode(bytes));
  const client=await SocketClient.connect(`${base}/${name.replace('socket_','')}`);
  for(const value of values) {
   await client.send(value);const event=await client.recv({timeoutMs:5000});assert.equal(event.type,'message');assert.deepEqual(event.value,value);
  }
  await assert.rejects(async()=>{await client.send('badjson');await client.recv({timeoutMs:5000});},/Invalid JSON/);
  client.terminate();
 }
 child=spawn(path.join(env.CARGO_TARGET_DIR,'debug/forklaunch-json-encoding-fixture'),[base],{stdio:'inherit'});
 const deadline=setTimeout(()=>child.kill('SIGKILL'),15000);
 try {const [code]=await once(child,'exit');assert.equal(code,0);}finally{clearTimeout(deadline);}
 assert.equal(messages,16);
 console.log('PASS imported JSON wire encoding, both compiled clients, 16 string/null roundtrips, exact quoted wire bytes and malformed-peer rejection');
} finally {
 if(child?.exitCode===null)child.kill('SIGKILL');
 for(const socket of sockets)socket.terminate();
 if(server)await new Promise(resolve=>server.close(resolve));
 await rm(temporary,{recursive:true,force:true});
}
