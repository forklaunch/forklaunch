// Compile all supported imported dialects and verify their codecs on live peers.
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
const temporary=await mkdtemp(path.join(tmpdir(),'fl-asyncapi-dialects-'));
const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_TARGET_DIR:path.resolve(root,process.env.CARGO_TARGET_DIR??'framework-rs/target')};
const dialects=[['draft-04','http://json-schema.org/draft-04/schema#'],['draft-06','http://json-schema.org/draft-06/schema#'],['draft-07','http://json-schema.org/draft-07/schema#'],['2019-09','https://json-schema.org/draft/2019-09/schema'],['2020-12','https://json-schema.org/draft/2020-12/schema']];
const document={asyncapi:'3.0.0',info:{title:'Dialects',version:'1'},defaultContentType:'application/json',servers:{local:{host:'localhost',protocol:'ws'}},channels:{},operations:{}};
for (const [index,[version,uri]] of dialects.entries()) {
 const name=`dialect${index}`;
 const bound=version==='draft-04'?{type:'number',minimum:0,exclusiveMinimum:true}:{type:'number',exclusiveMinimum:0};
 document.channels[name]={address:`/${name}`,messages:{value:{payload:{schemaFormat:`application/schema+json;version=${version}`,schema:{$schema:uri,type:'object',required:['value'],properties:{value:bound},additionalProperties:false}}}}};
 for(const action of ['receive','send']) document.operations[`${name}_${action}`]={action,channel:{$ref:`#/channels/${name}`},messages:[{$ref:`#/channels/${name}/messages/value`}]};
}
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
  await writeFile(path.join(temporary,`src/${name}.ts`),generated.typescript[name]);
  await writeFile(path.join(temporary,`src/${name}.rs`),generated.rust[name]);
 }
 await writeFile(path.join(temporary,'src/lib.rs'),modules.map(name=>`pub mod ${name};`).join('\n'));
 await writeFile(path.join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',esModuleInterop:true,types:['node'],outDir:'dist',rootDir:'src'},include:['src/*.ts']}));
 execFileSync(process.env.TSC_BIN,['--project',path.join(temporary,'tsconfig.json')],{stdio:'inherit',timeout:60000});
 await writeFile(path.join(temporary,'Cargo.toml'),`[package]\nname="forklaunch-imported-dialects-fixture"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\nforklaunch-ws={path=${JSON.stringify(path.join(framework,'crates/ws'))},features=["transport","json-schema"]}\nserde={version="1",features=["derive"]}\nserde_json="1"\ntokio={version="1",features=["net","macros","rt-multi-thread"]}\ntokio-tungstenite="0.30"\n`);
 await writeFile(path.join(temporary,'src/main.rs'),`use forklaunch_ws::transport::{SocketEvent,SocketOptions};
#[tokio::main] async fn main() {
let base=std::env::args().nth(1).unwrap();
${modules.map(name=>`{
use forklaunch_imported_dialects_fixture::${name}::SocketClient;
let (mut client,_)=SocketClient::connect(format!("{}/{}",base,"${name.replace('socket_','')}"),SocketOptions::default()).await.unwrap();
for value in [0,-1] {let message=serde_json::from_value(serde_json::json!({"value":value})).unwrap();assert!(client.send(&message).await.is_err());}
let message=serde_json::from_value(serde_json::json!({"value":1})).unwrap();client.send(&message).await.unwrap();
match client.recv().await.unwrap().unwrap(){SocketEvent::Message(value)=>assert_eq!(serde_json::to_value(value).unwrap()["value"].as_f64(),Some(1.0)),_=>panic!("expected message")}
client.close(1000,None).await.unwrap();
}`).join('\n')}
println!("PASS Rust imported dialect constraints and live messages");
}`);
 execFileSync('cargo',['build','--quiet','--manifest-path',path.join(temporary,'Cargo.toml')],{cwd:root,env,stdio:'inherit',timeout:300000});
 server=new WebSocketServer({host:'127.0.0.1',port:0});await once(server,'listening');
 let messages=0;
 server.on('connection',socket=>{sockets.add(socket);socket.on('close',()=>sockets.delete(socket));socket.on('message',bytes=>{assert.deepEqual(JSON.parse(bytes.toString()),{value:1});messages++;socket.send(bytes,{binary:false});});});
 const base=`ws://127.0.0.1:${server.address().port}`;
 for(const name of modules) {
  const {SocketClient}=await import(pathToFileURL(path.join(temporary,`dist/${name}.js`)));
  const client=await SocketClient.connect(`${base}/${name.replace('socket_','')}`);
  for(const value of [0,-1]) await assert.rejects(client.send({value}));
  await client.send({value:1});const event=await client.recv({timeoutMs:5000});assert.equal(event.type,'message');assert.deepEqual(event.value,{value:1});
  await client.close(1000);
 }
 child=spawn(path.join(env.CARGO_TARGET_DIR,'debug/forklaunch-imported-dialects-fixture'),[base],{stdio:'inherit'});
 const deadline=setTimeout(()=>child.kill('SIGKILL'),15000);
 try {const [code]=await once(child,'exit');assert.equal(code,0);}finally{clearTimeout(deadline);}
 assert.equal(messages,10);
 console.log('PASS five imported JSON Schema dialects, strict TypeScript compilation, Rust compilation, both live clients, 20 invalid-bound rejections and unchanged source document');
} finally {
 if(child?.exitCode===null)child.kill('SIGKILL');
 for(const socket of sockets)socket.terminate();
 if(server)await new Promise(resolve=>server.close(resolve));
 await rm(temporary,{recursive:true,force:true});
}
