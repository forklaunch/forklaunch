import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {randomInt} from 'node:crypto';
import {probeMcp} from './rust_mcp_probe.mjs';
const [binary,...options]=process.argv.slice(2);assert.ok(binary);
assert.ok(options.every(option=>option==='--verify-sdk-discovery'),'Unknown startup probe option');
const verifyDiscovery=options.includes('--verify-sdk-discovery');
for(const config of [{MCP_ENABLED:'true'},{MCP_ENABLED:'false'},{MCP_ENABLED:'true',MCP_PATH:'/tools'},{MCP_ENABLED:'true',MCP_LISTENER:'separate',MCP_PORT:'0'},{MCP_ENABLED:'true',MCP_LISTENER:'separate',MCP_PATH:'/tools',PORT:String(await freePortPair())},{MCP_ENABLED:'true',MCP_PORT:'0',MCP_HOST:'localhost'}]) {
  const env={...Object.fromEntries(Object.entries(process.env).filter(([key])=>!key.startsWith('OTEL_')&&!key.startsWith('MCP_'))),HOST:'127.0.0.1',PORT:'0',RUST_LOG:'info',MCP_ENABLED:'true',PROTOCOL:'http',VERSION:'v1',DOCS_PATH:'/docs',...config};
  const child=spawn(binary,[],{env,stdio:['ignore','pipe','pipe']});let output='',errors='',closed=false;
  child.stdout.on('data',d=>output+=d);child.stderr.on('data',d=>errors+=d);const exit=new Promise(resolve=>child.once('close',code=>{closed=true;resolve(code);}));
  try {
    let base;for(let i=0;i<200;i++){base=output.match(/listening on (http:\/\/127\.0\.0\.1:\d+)/)?.[1];if(base)break;assert.ok(!closed,errors);await new Promise(resolve=>setTimeout(resolve,25));}assert.ok(base,errors);
    if(verifyDiscovery)await sdkDiscovery(base);
    if(config.MCP_ENABLED==='false'){assert.equal((await fetch(`${base}/health`)).status,200);assert.equal((await fetch(`${base}/mcp`,{method:'POST'})).status,404);}
    else{
      const separate=config.MCP_LISTENER==='separate'||config.MCP_PORT!==undefined;
      let mcpBase=base;
      if(separate){
        for(let i=0;i<200;i++){mcpBase=output.match(/MCP listening on (http:\/\/\S+)/)?.[1];if(mcpBase)break;assert.ok(!closed,errors);await new Promise(resolve=>setTimeout(resolve,25));}
        assert.ok(mcpBase,errors);assert.notEqual(mcpBase,base);
        if(config.MCP_PORT===undefined)assert.equal(new URL(mcpBase).port,String(Number(new URL(base).port)+2000));
        assert.equal((await fetch(`${base}${config.MCP_PATH??'/mcp'}`,{method:'POST'})).status,404);
        assert.equal((await fetch(`${mcpBase}/health`)).status,404);
      }
      await probeMcp(mcpBase,[],config.MCP_PATH??'/mcp',base);
      if(config.MCP_PATH)assert.equal((await fetch(`${mcpBase}/mcp`,{method:'POST'})).status,404);
    }
  } finally {
    if(!closed)child.kill('SIGTERM');const timer=setTimeout(()=>child.kill('SIGKILL'),10000);try{assert.equal(await exit,0,errors);}finally{clearTimeout(timer);}
  }
}
for(const config of [
  {MCP_ENABLED:'invalid'},
  {MCP_LISTENER:'invalid'},
  {MCP_PORT:'65536'},
  {MCP_LISTENER:'shared',MCP_PORT:'0'},
  {MCP_HOST:'bad host',MCP_PORT:'0'},
])await rejectedStartup(config);
const occupied=createServer();await new Promise(resolve=>occupied.listen(0,'127.0.0.1',resolve));
try{await rejectedStartup({MCP_LISTENER:'separate',MCP_PORT:String(occupied.address().port)});}
finally{await new Promise(resolve=>occupied.close(resolve));}
console.log('PASS generated service HTTP/MCP startup, enabled/disabled/custom endpoints, shared/separate listeners, explicit/ephemeral ports, TypeScript +2000 fallback, atomic startup rejection and graceful shutdown');
if(verifyDiscovery)console.log('PASS registry-installed SDK discovery: canonical OpenAPI, stable hash and checked health operation client path across every listener configuration');
async function sdkDiscovery(base){
  const documentResponse=await fetch(`${base}/openapi.json`);
  assert.equal(documentResponse.status,200);
  assert.match(documentResponse.headers.get('content-type'),/^application\/json/);
  const document=await documentResponse.json(),operation=document.paths['/health'].get['x-forklaunch-operation'];
  assert.equal(operation.format,'forklaunch.sdk-operation.v1');
  assert.equal(operation.path,'/health');assert.equal(operation.method,'GET');
  assert.ok(Array.isArray(operation.clientPath)&&operation.clientPath.length>0);
  for(const segment of operation.clientPath)assert.match(segment,/^[A-Za-z_$][A-Za-z0-9_$]*$/);
  const hashes=[];
  for(let attempt=0;attempt<2;attempt++){
    const response=await fetch(`${base}/openapi.json-hash`);assert.equal(response.status,200);
    const hash=await response.text();assert.match(hash,/^[0-9a-f]{16}$/);hashes.push(hash);
  }
  assert.equal(hashes[0],hashes[1]);
}
async function freePortPair(){
  for(let i=0;i<100;i++){
    const http=createServer(),mcp=createServer(),port=randomInt(20000,40000);
    try{
      await new Promise((resolve,reject)=>{http.once('error',reject);http.listen(port,'127.0.0.1',resolve);});
      await new Promise((resolve,reject)=>{mcp.once('error',reject);mcp.listen(port+2000,'127.0.0.1',resolve);});
      return port;
    }catch(error){if(error.code!=='EADDRINUSE')throw error;}
    finally{await Promise.all([http,mcp].map(server=>new Promise(resolve=>server.close(resolve))));}
  }
  throw Error('Could not find free HTTP/MCP port pair');
}

async function rejectedStartup(config){
  const env={...Object.fromEntries(Object.entries(process.env).filter(([key])=>!key.startsWith('OTEL_')&&!key.startsWith('MCP_'))),HOST:'127.0.0.1',PORT:'0',RUST_LOG:'info',MCP_ENABLED:'true',PROTOCOL:'http',VERSION:'v1',DOCS_PATH:'/docs',...config};
  const child=spawn(binary,[],{env,stdio:['ignore','pipe','pipe']});let output='',errors='';
  child.stdout.on('data',d=>output+=d);child.stderr.on('data',d=>errors+=d);
  const exit=new Promise(resolve=>child.once('close',resolve)),timer=setTimeout(()=>child.kill('SIGKILL'),5000);
  try{const status=await exit;assert.equal(typeof status,'number',`Startup did not fail promptly: ${errors}`);assert.notEqual(status,0,JSON.stringify(config));}
  finally{clearTimeout(timer);}
  const http=output.match(/listening on (http:\/\/127\.0\.0\.1:\d+)/)?.[1];
  if(http){
    const probe=createServer();await new Promise((resolve,reject)=>{probe.once('error',reject);probe.listen(Number(new URL(http).port),'127.0.0.1',resolve);});await new Promise(resolve=>probe.close(resolve));
  }
}
