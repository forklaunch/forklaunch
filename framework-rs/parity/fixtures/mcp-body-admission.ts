import {createServer} from 'node:http';
import {SchemaValidator} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {generateMcpServer} from './source/core/src/http/mcpGenerator/mcpGenerator';
let wire:any;
const http=createServer((req,res)=>{const chunks:Buffer[]=[];req.on('data',chunk=>chunks.push(chunk));req.on('end',()=>{wire={body:Buffer.concat(chunks).toString(),length:req.headers['content-length']??null,media:req.headers['content-type']??null};res.writeHead(200,{'content-type':'application/json'});res.end('{"ok":true}');});});
await new Promise<void>(resolve=>http.listen(0,'127.0.0.1',resolve));
const port=(http.address() as any).port;
const routes:any[]=[];
for(const kind of ['json','text'])for(const form of ['required','optional','nullable','nullish','default'])for(const method of ['post','delete']){
 let schema=kind==='json'?z.object({flag:z.string()}).strict():z.string();
 if(form==='optional')schema=schema.optional() as any;
 if(form==='nullish')schema=schema.nullish() as any;
 if(form==='nullable')schema=schema.nullable() as any;
 if(form==='default')schema=schema.default(kind==='json'?{flag:'default'}:'default') as any;
 routes.push({method,path:'/wire',contractDetails:{name:kind+'_'+form+'_'+method,summary:'Admission',body:{[kind]:schema},responses:{200:{json:z.unknown()}},options:{mcp:true}}});
}
const free=createServer();await new Promise<void>(resolve=>free.listen(0,'127.0.0.1',resolve));const mcpPort=(free.address() as any).port;await new Promise<void>(resolve=>free.close(()=>resolve()));
const mcp=generateMcpServer(SchemaValidator(),'http','127.0.0.1',port,'1.0.0',{basePath:'/',routers:[],routes} as any);
await mcp.start({transportType:'httpStream',httpStream:{host:'127.0.0.1',port:mcpPort,endpoint:'/mcp',stateless:true,enableJsonResponse:true}});
async function rpc(method:string,params:any){const response=await fetch(`http://127.0.0.1:${mcpPort}/mcp`,{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});return response.json();}
try{
 const listed=await rpc('tools/list',{}); const rows:any[]=[];
 for(const route of routes)for(const [label,args] of [['omitted',{}],['no_arguments',undefined],['undefined_body',{body:undefined}],['null_arguments',null],['null',{body:null}],['empty',{body:''}],['value',{body:route.contractDetails.name.startsWith('json')?{flag:'ok'}:'ok'}]] as any){
  wire=undefined;const result=await rpc('tools/call',{name:route.contractDetails.name,arguments:args});
  rows.push({name:route.contractDetails.name,input:label,args,wire:wire??null,result});
 }
 await (await import('node:fs/promises')).writeFile('observations.json',JSON.stringify({tools:listed.result.tools,rows}));
}finally{await mcp.stop();http.closeAllConnections();await new Promise<void>(resolve=>http.close(()=>resolve()));}
