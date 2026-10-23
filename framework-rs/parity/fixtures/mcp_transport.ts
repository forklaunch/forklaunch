import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFileSync,writeFileSync} from 'node:fs';
import {once} from 'node:events';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {InMemoryTransport} from '@modelcontextprotocol/sdk/inMemory.js';
import {z} from 'zod/v3';
import {ZodSchemaValidator} from '@forklaunch/validator/zod';
import {generateMcpServer} from '@fixture/mcp';
const cases=JSON.parse(readFileSync('input.json','utf8'));
const captures=new Map<number,{body:string,contentType:string,url:string,method:string}>();
const http=createServer(async(req,res)=>{
 const chunks=[];for await(const chunk of req)chunks.push(chunk);
 captures.set(Number(new URL(req.url??'/', 'http://fixture').pathname.split('/').pop()),{url:req.url??'',method:req.method??'',body:Buffer.concat(chunks).toString('base64'),contentType:req.headers['content-type']??''});
 res.writeHead(200,{'content-type':'application/json'});res.end('{"ok":true}');
});
http.listen(0,'127.0.0.1');await once(http,'listening');
const port=(http.address() as {port:number}).port;
const typed={json:'json',text:'text',file:'file',binary:'file',multipart:'multipartForm',form:'urlEncodedForm'};
const routes=cases.map((input:any,index:number)=>({method:'post',path:`/body/${index}`,contractDetails:{name:`Body${index}`,summary:'Body transport',body:{[typed[input.kind as keyof typeof typed]]:input.kind==='json'&&typeof input.arguments.body==='string'?z.literal(input.arguments.body):z.unknown(),...(input.contentType?{contentType:input.contentType}:{})},responses:{200:{json:{ok:z.boolean()}}},options:{mcp:true}}}));
routes.push({method:'post',path:'/body/1000',contractDetails:{name:'Versioned',summary:'Named versions',versions:{
 '2026-01':{body:{json:{revision:z.literal('old')}},responses:{200:{json:{ok:z.boolean()}}}},
 '2026-10':{body:{json:{revision:z.literal('new')}},responses:{200:{json:{ok:z.boolean()}}}}
}}} as any);
routes.push({method:'get',path:'/body/1001',contractDetails:{name:'VersionedRead',summary:'Bodyless versions',versions:{
 '2026-01':{query:{revision:z.literal('old')},responses:{200:{json:{ok:z.boolean()}}}},
 '2026-10':{query:{revision:z.literal('new')},responses:{200:{json:{ok:z.boolean()}}}}
}}} as any);
const server=generateMcpServer(new ZodSchemaValidator(),'http','127.0.0.1',port,'1.0.0',{basePath:'/',routers:[],routes} as any);
const client=new Client({name:'parity',version:'1.0.0'});
const [clientTransport,serverTransport]=InMemoryTransport.createLinkedPair();
let session;
try{
 [session]=await Promise.all([server.connect(serverTransport),client.connect(clientTransport)]);
 const tools=await client.listTools();assert.equal(tools.tools.length,cases.length+4);
 for(const tool of tools.tools)assert.equal(tool.inputSchema.type,'object');
 const results=[];
 for(let index=0;index<cases.length;index++){
  const result=await client.callTool({name:`Body${index}`,arguments:cases[index].arguments});
  const capture=captures.get(index);
  results.push({error:result.isError===true,captured:Boolean(capture),...capture,...(result.isError?{detail:result.content}:{})});
  if(!result.isError)assert.deepEqual(JSON.parse((result.content as any)[0].text),{ok:true});
 }
 for(const [version,revision] of [['2026-01','old'],['2026-10','new']]) {
  for(const [name,args] of [['Versioned',{body:{revision}}],['VersionedRead',{query:{revision}}]] as const) {
   const result=await client.callTool({name:`${name} [v${version}]`,arguments:args});
   assert.notEqual(result.isError,true,JSON.stringify(result));
   assert.deepEqual(JSON.parse((result.content as any)[0].text),{ok:true});
   if(name==='Versioned')assert.equal(Buffer.from(captures.get(1000)!.body,'base64').toString(),JSON.stringify({revision}));
   else assert.equal(captures.get(1001)!.url,`/body/1001?revision=${revision}`);
  }
 }
 await assert.rejects(client.callTool({name:'Versioned [v2026-01]',arguments:{body:{revision:'new'}}}), /parameter validation failed/);
 writeFileSync('actual.json',JSON.stringify(results));
}finally{
 await client.close();await session?.close();await serverTransport.close();await server.stop();
 http.closeAllConnections();await new Promise<void>((resolve,reject)=>http.close(error=>error?reject(error):resolve()));
}
