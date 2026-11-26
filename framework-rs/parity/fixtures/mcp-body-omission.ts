import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {SchemaValidator} from '@forklaunch/validator/zod';
import {z} from 'zod/v3';
import {forklaunchExpress} from '@forklaunch/express';
import {OpenTelemetryCollector} from '@forklaunch/core/http';
import {generateMcpServer} from './source/core/src/http/mcpGenerator/mcpGenerator';
let wire:any;
const app=forklaunchExpress(SchemaValidator(),new OpenTelemetryCollector('admission','silent'),{mcp:false});
app.use((req:any,_res:any,next:any)=>{const chunks:Buffer[]=[]; wire={body:'',length:req.headers['content-length']??null,media:req.headers['content-type']??null};req.on('data',(chunk:Buffer)=>chunks.push(chunk));req.on('end',()=>{wire.body=Buffer.concat(chunks).toString();});next();});
const routes:any[]=[];
for(const kind of ['json','text'])for(const form of ['required','optional','nullable','nullish','default'])for(const method of ['post','delete']){
 let schema=kind==='json'?z.object({flag:z.string()}):z.string();
 if(form==='optional')schema=schema.optional() as any;
 if(form==='nullable')schema=schema.nullable() as any;
 if(form==='nullish')schema=schema.nullish() as any;
 if(form==='default')schema=schema.default(kind==='json'?{flag:'default'}:'default') as any;
 routes.push({method,path:'/'+kind+'_'+form+'_'+method,contractDetails:{name:kind+'_'+form+'_'+method,summary:'Admission',body:{[kind]:schema},responses:{200:{json:z.unknown()}},options:{mcp:true}}});
 const route=routes.at(-1);(app as any)[method](route.path,route.contractDetails,(req:any,res:any)=>res.json({parsed:req.body??null}));
}
const http=app.listen(0,'127.0.0.1');await new Promise<void>(resolve=>http.on('listening',resolve));
const port=(http.address() as any).port;
const free=createServer();await new Promise<void>(resolve=>free.listen(0,'127.0.0.1',resolve));const mcpPort=(free.address() as any).port;await new Promise<void>(resolve=>free.close(()=>resolve()));
const mcp=generateMcpServer(SchemaValidator(),'http','127.0.0.1',port,'1.0.0',{basePath:'/',routers:[],routes} as any);
await mcp.start({transportType:'httpStream',httpStream:{host:'127.0.0.1',port:mcpPort,endpoint:'/mcp',stateless:true,enableJsonResponse:true}});
async function rpc(method:string,params:any){const response=await fetch(`http://127.0.0.1:${mcpPort}/mcp`,{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params})});return response.json();}
try{
 const listed=await rpc('tools/list',{}); assert.equal(listed.result.tools.length,20);
 let count=0;
 for(const route of routes)for(const [label,args] of [['omitted',{}],['no_arguments',undefined],['undefined_body',{body:undefined}],['null_arguments',null],['null',{body:null}],['empty',{body:''}],['value',{body:route.contractDetails.name.startsWith('json')?{flag:'ok'}:'ok'}]] as any){
  wire=undefined;const result=await rpc('tools/call',{name:route.contractDetails.name,arguments:args});
  const [kind,form,method]=route.contractDetails.name.split('_');
  const missing=['omitted','no_arguments','undefined_body'].includes(label);
  const nullable=['nullable','nullish'].includes(form);
  const admitted=label!=='null_arguments' && !(missing&&['required','nullable'].includes(form)) && !(label==='null'&&!nullable) && !(label==='empty'&&kind==='json');
  if(!admitted){assert.equal(wire,undefined);assert.ok(result.error);}
  else {
   assert.ok(wire);
   const defaulted=missing&&form==='default';
   const empty=!defaulted&&(missing||(kind==='text'&&['null','empty'].includes(label)));
   assert.equal(wire.media,kind==='json'?'application/json':'text/plain');
   if(empty){assert.equal(wire.body,'');assert.equal(wire.length,method==='post'?'0':null);}
   const invalid=(kind==='json'&&(label==='null'||(empty&&method==='post'))) || (kind==='text'&&empty&&method==='delete'&&!['optional','nullish','default'].includes(form));
   assert.equal(!!result.result.isError,invalid,JSON.stringify({name:route.contractDetails.name,label,result}));
   if(!invalid){const parsed=JSON.parse(result.result.content[0].text).parsed;assert.deepEqual(parsed,(defaulted||(empty&&method==='delete'&&form==='default'))?(kind==='json'?{flag:'default'}:'default'):empty?(method==='delete'?null:''):kind==='json'?{flag:'ok'}:'ok');}
  }
  count++;
 }
 assert.equal(count,140);console.log('PASS 140 actual registered FastMCP/Express omission, null, default, framing and downstream schema outcomes');
}catch(error){console.error("OMISSION ASSERTION",error);throw error;}finally{await mcp.stop();http.closeAllConnections();await new Promise<void>(resolve=>http.close(()=>resolve()));}
