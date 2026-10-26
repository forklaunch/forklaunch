import assert from 'node:assert/strict';
import {spawn,execFileSync} from 'node:child_process';
import {createRequire} from 'node:module';
import {once} from 'node:events';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
process.env.RUSTUP_TOOLCHAIN = 'stable';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {statics:v,validator,load}=actualValidator(root,(spec,file,load)=>spec==='@forklaunch/core/ws'?load(path.join(root,'framework/core/src/ws/webSocketLike.ts')):undefined);
const {ForklaunchWebSocket}=load(path.join(root,'framework/ws/src/webSocket.ts'));
const {ForklaunchWebSocketServer}=load(path.join(root,'framework/ws/src/webSocketServer.ts'));
const external=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {WebSocket,WebSocketServer}=external('ws');
const schemas={
 clientMessages:{request:{shape:v.schemify({message:v.string.min(1)})}},
 serverMessages:{reply:{shape:v.schemify({accepted:v.string})}},
 ping:{shape:v.schemify({ts:v.number.innerType().int().min(1)})},
 pong:{shape:v.schemify({ts:v.number.innerType().int().min(1)})},
 closeReason:{reason:{shape:v.schemify({reason:v.string})}}
};
execFileSync('cargo',['build','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-ws','--all-features','--example','websocket_runtime'],{cwd:root,stdio:'inherit',timeout:300000});
const target=path.resolve(root,process.env.CARGO_TARGET_DIR??'framework-rs/target');
const binary=path.join(target,'debug/examples/websocket_runtime');
const children=new Set(),sockets=new Set(),servers=new Set();
const deadline=setTimeout(()=>{console.error('WebSocket interoperability deadline exceeded');for(const child of children)child.kill();for(const socket of sockets)socket.terminate();process.exitCode=1;},30000);
function native(...args){
 const child=spawn(binary,args,{stdio:['ignore','pipe','pipe']});children.add(child);
 let output='',error='';child.stdout.on('data',chunk=>{output+=chunk;});child.stderr.on('data',chunk=>{error+=chunk;});
 const done=new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',code=>{children.delete(child);code===0?resolve(output):reject(new Error(`native ${args[0]} exited ${code}: ${error}\n${output}`));});});
 // Attach a handler immediately; callers still await the original rejection.
 done.catch(()=>{});
 return {child,done,url:()=>new Promise((resolve,reject)=>{
  const check=()=>{const match=output.match(/ws:\/\/[^\s]+/);if(match){child.stdout.off('data',check);resolve(match[0]);}};
  child.stdout.on('data',check);done.then(()=>{if(!output.includes('ws://'))reject(new Error('native server produced no URL'));},reject);check();
 })};
}
async function verifyBooleanMessageSchemas(){
 const rawValidator={
  union:values=>({anyOf:values}),
  parse:(schema,value)=>schema===false
   ?{ok:false,errors:[{path:[],message:'false schema rejects every value'}]}
   :{ok:true,value}
 };
 for(const shape of [false,true,undefined]){
  const eventSchemas=shape===undefined?{}:{clientMessages:{raw:{shape}},serverMessages:{raw:{shape}}};
  const server=new ForklaunchWebSocketServer(rawValidator,eventSchemas,{host:'127.0.0.1',port:0});servers.add(server);
  await once(server,'listening');const accepted=once(server,'connection');
  const client=new ForklaunchWebSocket(rawValidator,eventSchemas,`ws://127.0.0.1:${server.address().port}`);sockets.add(client);
  await once(client,'open');const [socket]=await accepted;sockets.add(socket);
  for(const [sender,receiver] of [[client,socket],[socket,client]]){
   let delivered=0;const errors=[];
   const observed=new Promise(resolve=>{
    receiver.once('message',value=>{delivered++;resolve(value);});
    receiver.once('error',error=>{errors.push(error);resolve(error);});
   });
   // Bypass outgoing validation to exercise incoming policy on both surfaces.
   WebSocket.prototype.send.call(sender,JSON.stringify({value:'water 水'}));
   const value=await observed;
   if(shape===false){
    assert.equal(delivered,0);assert.equal(errors.length,1);assert.match(value.message,/false schema rejects/);
    assert.throws(()=>sender.send({value:'water 水'}));
   }else{
    assert.equal(delivered,1);assert.equal(errors.length,0);
    assert.deepEqual(shape===undefined?JSON.parse(value.toString()):JSON.parse(JSON.stringify(value)),{value:'water 水'});
   }
   assert.equal(receiver.readyState,WebSocket.OPEN);
   receiver.removeAllListeners('message');receiver.removeAllListeners('error');
  }
  const closed=once(socket,'close');client.close();await closed;
 }
 console.log('PASS actual TypeScript client/server boolean message schemas: false rejects before typed delivery, true decodes and absent schemas preserve raw frames');
}
async function verifyProtocolFailures(){
 const mismatched={...schemas,pong:{shape:v.schemify({ts:v.number.innerType().int().min(100)})}};
 for(const kind of ['pong','close','stalled']){
  const server=new ForklaunchWebSocketServer(validator,kind==='close'?schemas:mismatched,{host:'127.0.0.1',port:0});servers.add(server);
  await once(server,'listening');
  let typedPings=0,typedCloses=0;const errors=[];
  const accepted=new Promise(resolve=>server.once('connection',socket=>{
   sockets.add(socket);socket.on('error',error=>errors.push(error));
   socket.on('ping',()=>typedPings++);socket.on('close',()=>typedCloses++);resolve(socket);
  }));
  const client=new WebSocket(`ws://127.0.0.1:${server.address().port}`);sockets.add(client);
  await once(client,'open');const socket=await accepted;
  const closed=new Promise(resolve=>WebSocket.prototype.on.call(socket,'close',resolve));
  const remoteClosed=once(client,'close');
  if(kind==='stalled')client._socket.pause();
  const started=Date.now();
  if(kind==='close')client.close(1000,'invalid typed reason');else client.ping(JSON.stringify({ts:1}));
  await closed;
  if(kind==='stalled'){
   assert.ok(Date.now()-started>=4500,'unresponsive close peer released too early');
   assert.ok(Date.now()-started<10000,'protocol failure close deadline was not bounded');
   client._socket.resume();
  }
  const [code]=await remoteClosed;assert.equal(code,1008);
  assert.equal(errors.length,1);assert.match(errors[0].message,/validation failed/);
  assert.equal(typedPings,0);assert.equal(typedCloses,0);
 }
 for(const autoPong of [true,false]){
  const server=new WebSocketServer({host:'127.0.0.1',port:0});servers.add(server);await once(server,'listening');
  const accepted=once(server,'connection');
  const client=new ForklaunchWebSocket(validator,mismatched,`ws://127.0.0.1:${server.address().port}`,undefined,{autoPong});sockets.add(client);
  const errors=[];client.on('error',error=>errors.push(error));
  await once(client,'open');const [socket]=await accepted;sockets.add(socket);
  assert.throws(()=>client.pong(Buffer.from('invalid')));assert.throws(()=>client.close(1000,Buffer.from('invalid')));
  assert.equal(client.readyState,WebSocket.OPEN);assert.equal(errors.length,0);
  const closed=once(socket,'close');
  if(autoPong){
   let typedPings=0,typedCloses=0;client.on('ping',()=>typedPings++);client.on('close',()=>typedCloses++);
   socket.ping(JSON.stringify({ts:1}));assert.equal((await closed)[0],1008);
   assert.equal(errors.length,1);assert.equal(typedPings,0);assert.equal(typedCloses,0);
  }else{
   const ping=new Promise(resolve=>client.once('ping',resolve));socket.ping(JSON.stringify({ts:1}));
   assert.equal((await ping).ts,1);assert.equal(errors.length,0);
   client.close(1000,{reason:'done'});await closed;
  }
 }
 const server=new WebSocketServer({host:'127.0.0.1',port:0});servers.add(server);await once(server,'listening');
 const accepted=once(server,'connection');
 const client=new ForklaunchWebSocket(validator,schemas,`ws://127.0.0.1:${server.address().port}`);sockets.add(client);
 const failed=new Promise(resolve=>client.on('error',resolve));let closes=0;client.on('close',()=>closes++);
 await once(client,'open');const [socket]=await accepted;sockets.add(socket);socket.terminate();
 assert.match((await failed).message,/without a handshake/);assert.equal(closes,0);
 console.log('PASS actual TypeScript automatic-control failures: one error, 1008 before typed delivery, bounded unresponsive-peer close, manual validation, disabled pong and abnormal close');
}
try{
 const server=native('server');const url=await server.url();
 const denied=new WebSocket(url);sockets.add(denied);
 await new Promise((resolve,reject)=>{denied.on('unexpected-response',(_,response)=>{try{assert.equal(response.statusCode,401);response.resume();denied.terminate();resolve();}catch(error){reject(error);}});denied.on('open',()=>reject(new Error('unauthorized upgrade succeeded')));denied.on('error',()=>{});});
 const client=new ForklaunchWebSocket(validator,schemas,url,{headers:{authorization:'Bearer fixture-token'}});sockets.add(client);
 let messageCount=0,pingCount=0;
 const closed=new Promise((resolve,reject)=>{
  client.on('error',reject);
  client.on('message',(message,isBinary)=>{try{messageCount++;assert.deepEqual(JSON.parse(JSON.stringify(message)),{accepted:'水 🦀'});assert.equal(isBinary,false);}catch(error){reject(error);}});
  client.on('ping',data=>{try{pingCount++;assert.equal(data.ts,42);client.pong({ts:43});}catch(error){reject(error);}});
  client.on('close',(code,reason)=>{try{assert.equal(code,1000);assert.equal(reason.reason,'done');resolve();}catch(error){reject(error);}});
 });
 await once(client,'open');
 assert.throws(()=>client.send({message:''}));assert.throws(()=>client.ping({ts:0}));
 client.send({message:'hello 水',extra:'stripped'});
 await closed;assert.equal(messageCount,1);assert.equal(pingCount,1);assert.match(await server.done,/PASS native server/);
 const tsServer=new ForklaunchWebSocketServer(validator,schemas,{host:'127.0.0.1',port:0,path:'/chat',verifyClient:({req},done)=>done(req.headers.authorization==='Bearer fixture-token',401,'Unauthorized')});servers.add(tsServer);
 const connection=new Promise((resolve,reject)=>{
  tsServer.on('error',reject);
  tsServer.on('connection',socket=>{
   sockets.add(socket);
   socket.on('error',reject);
   socket.on('message',(message,isBinary)=>{try{assert.equal(message.message,'hello 水');assert.equal(isBinary,false);socket.send({accepted:'水 🦀'});}catch(error){reject(error);}});
   socket.on('ping',data=>{try{assert.equal(data.ts,42);socket.pong({ts:43});socket.close(1000,{reason:'done'});}catch(error){reject(error);}});
   socket.on('close',(code,reason)=>{try{assert.equal(code,1000);assert.equal(reason.reason,'done');resolve();}catch(error){reject(error);}});
  });
 });
 await once(tsServer,'listening');
 const peer=native('client',`ws://127.0.0.1:${tsServer.address().port}/chat`);
 await Promise.all([connection,peer.done]);
 await verifyBooleanMessageSchemas();
 await verifyProtocolFailures();
 console.log('PASS actual TypeScript/native Rust WebSockets in both directions: upgrade authorization, typed Unicode messages, outgoing validation, automatic and explicit pong, typed close handshake');
}finally{
 clearTimeout(deadline);
 for(const socket of sockets)if(socket.readyState!==WebSocket.CLOSED)socket.terminate();
 for(const server of servers)await new Promise(resolve=>server.close(resolve));
 for(const child of children)child.kill();
}
