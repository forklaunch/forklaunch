import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mcpRpc} from './rust_mcp_probe.mjs';
const [binary]=process.argv.slice(2);
const child=spawn(binary,[],{env:{...process.env,HOST:'127.0.0.1',PORT:'0',RUST_LOG:'info',MCP_ENABLED:'true',PROTOCOL:'http',VERSION:'v1',DOCS_PATH:'/docs'},stdio:['ignore','pipe','pipe']});
let output='',errors='';
child.stdout.on('data',chunk=>output+=chunk);child.stderr.on('data',chunk=>errors+=chunk);
const closed=new Promise(resolve=>child.once('close',resolve));
try {
 let base;
 for(let attempt=0;attempt<200;attempt++) {
  base=output.match(/listening on (http:\/\/127\.0\.0\.1:\d+)/)?.[1];
  if(base)break;
  assert.equal(child.exitCode,null,errors);
  await new Promise(resolve=>setTimeout(resolve,25));
 }
 assert.ok(base,errors);
 const response=await fetch(`${base}/jobs`,{method:'POST',headers:{'content-type':'application/json','x-tenant-id':'00000000-0000-0000-0000-000000000001'},body:JSON.stringify({resourceId:'00000000-0000-0000-0000-000000000001',revision:1,message:'untrusted-job'})});
 assert.equal(response.status,401,'ordinary producer must reject unverified identity before queue access');
 const result=await mcpRpc(base,'tools/call',{name:'enqueueWork',arguments:{body:{resourceId:'00000000-0000-0000-0000-000000000001',revision:1,message:'untrusted-job'}}});
 assert.equal(result.isError,true,'MCP must preserve producer authorization');
 console.log('PASS producer HTTP/MCP fail-closed authorization and untrusted tenant rejection');
} finally {
 child.kill('SIGTERM');const timer=setTimeout(()=>child.kill('SIGKILL'),10000);
 try { assert.equal(await closed,0,errors); } finally {clearTimeout(timer);}
}
