// Actual Chromium proof against the emitted browser client (no Node WebSocket polyfill).
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
const root=path.resolve(process.argv[2]);
const {createServer}=await import(pathToFileURL(process.env.FORKLAUNCH_TEST_VITE));
const {chromium}=await import(pathToFileURL(process.env.FORKLAUNCH_TEST_PLAYWRIGHT));
const require=createRequire(path.join(root,'package.json'));
const {WebSocketServer}=require('ws');
await writeFile(path.join(root,'index.html'),'<!doctype html><title>Browser socket proof</title>');
const vite=await createServer({root,configFile:false,server:{host:'127.0.0.1',port:0},logLevel:'error'});
const sockets=new WebSocketServer({noServer:true});
let cookieSeen=false, received=0;
vite.httpServer.on('upgrade',(req,socket,head)=>{
  if(!req.url.startsWith('/socket'))return;
  sockets.handleUpgrade(req,socket,head,ws=>{
    cookieSeen ||= req.headers.cookie?.includes('browser-proof=present') ?? false;
    const mode=new URL(req.url,'http://localhost').searchParams.get('mode');
    ws.on('message',bytes=>{received++; ws.send(bytes.toString());});
    if(mode==='datebad') setTimeout(()=>ws.send('{"at":"2025-02-30T12:00:00.000Z"}'),20);
    if(mode==='bad') setTimeout(()=>ws.send('{"message":42}'),20);
    if(mode==='large') setTimeout(()=>ws.send(JSON.stringify({message:'x'.repeat(1024)})),20);
    if(mode==='queue') setTimeout(()=>{ws.send('{"message":"a"}');ws.send('{"message":"b"}');},20);
    if(mode==='close') setTimeout(()=>ws.close(1000,'{"reason":"done"}'),20);
  });
});
let browser;
try{
  await vite.listen();
  const port=vite.httpServer.address().port;
  browser=await chromium.launch({headless:true, ...(process.env.FORKLAUNCH_TEST_BROWSER ? {executablePath:process.env.FORKLAUNCH_TEST_BROWSER} : {})});
  const page=await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}`);
  const results=await page.evaluate(async port=>{
    const {ForkLaunchSocketClient:Client}=await import('/client.ts');
    document.cookie='browser-proof=present; SameSite=Strict';
    const endpoint=`ws://127.0.0.1:${port}/socket`;
    const check=(value,label)=>{if(!value)throw Error(label);};
    const rejects=async(work,label)=>{let denied=false;try{await work();}catch{denied=true;}check(denied,label);};
    await rejects(()=>Client.connect('ws://example.com/socket'),'remote plaintext');
    const abort=new AbortController();
    const client=await Client.connect(endpoint,{signal:abort.signal});
    await client.send({message:'hello'});
    check((await client.recv()).value.message==='hello','round trip');
    await rejects(()=>client.send({message:42}),'outgoing type');
    await rejects(()=>client.send({message:'valid',toJSON(){return {message:42};}}),'serialized outgoing type');
    await rejects(()=>client.send({message:'\ud800'}),'unpaired surrogate');
    await rejects(()=>client.recv({timeoutMs:10}),'read timeout');
    await client.send({message:'after timeout'});
    check((await client.recv()).value.message==='after timeout','timeout preserves connection');
    const pending=client.recv(); abort.abort(); await rejects(()=>pending,'cancellation');
    for(const mode of ['bad','large','queue']){
      const other=await Client.connect(`${endpoint}?mode=${mode}`,{maxMessageBytes:128,maxQueuedEvents:1});
      if(mode==='queue')await new Promise(resolve=>setTimeout(resolve,80));
      await rejects(()=>other.recv({timeoutMs:1000}),mode);other.terminate();
    }
    const closing=await Client.connect(`${endpoint}?mode=close`);
    const event=await closing.recv({timeoutMs:1000});
    check(event.type==='close' && event.reason.reason==='done','typed close');
    check(await closing.recv()===null,'closed iterator');
    check(typeof client.ping==='undefined' && typeof client.upgradeResponse==='undefined','browser API capabilities');
    const {DateSocketClient}=await import('/date.ts');
    const dates=await DateSocketClient.connect(endpoint);
    await dates.send({at:new Date('2026-10-17T00:01:02.123Z')});
    const dateReply=(await dates.recv()).value.at;
    check(dateReply instanceof Date && dateReply.toISOString()==='2026-10-17T00:01:02.123Z','Date decode');
    await rejects(()=>dates.send({at:'2026-10-17T00:01:02.123Z'}),'Date type');
    await rejects(()=>dates.send({at:new Date(NaN)}),'invalid Date');
    dates.terminate();
    const invalidDate=await DateSocketClient.connect(`${endpoint}?mode=datebad`);
    await rejects(()=>invalidDate.recv({timeoutMs:1000}),'calendar validity');invalidDate.terminate();
    return {roundTrip:true,canonicalValidation:true,incomingValidation:true,limits:true,cancellation:true,typedClose:true,dateCodec:true};
  },port);
  assert(cookieSeen,'browser cookie handshake');
  assert.equal(received,3,'invalid outgoing values never reached peer');
  console.log(JSON.stringify({...results,cookieAuthentication:true,transport:'native Chromium WebSocket'}));
}finally{
  await browser?.close();
  for(const socket of sockets.clients)socket.terminate();
  await new Promise(resolve=>sockets.close(resolve));
  await vite.close();
}
