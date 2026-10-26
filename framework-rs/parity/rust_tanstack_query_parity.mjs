import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,symlinkSync,rmSync} from 'node:fs';
import {createServer} from 'node:http';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_TANSTACK_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_TANSTACK_DEPENDENCIES');
const require=createRequire(import.meta.url);
assert.equal(require(join(dependencies,'node_modules/@tanstack/query-core/package.json')).version,'5.104.2');
assert.equal(require(join(dependencies,'node_modules/@tanstack/react-query/package.json')).version,'5.104.2');
const {QueryClient,MutationObserver,isCancelledError}=require(join(dependencies,'node_modules/@tanstack/query-core'));
const temporary=mkdtempSync(join(tmpdir(),'fl-tanstack-parity-'));
const counts=new Map(),started=new Set(),closed=new Set();
const server=createServer(async(request,response)=>{
 const id=decodeURIComponent(request.url.split('/').at(-1));
 counts.set(id,(counts.get(id)??0)+1);started.add(id);
 if(id.startsWith('slow')){response.on('close',()=>closed.add(id));return;}
 if(id==='network'){request.socket.destroy();return;}
 if(id==='missing'||(id==='retry'&&counts.get(id)===1)){response.writeHead(id==='missing'?404:500,{'content-type':'text/plain'});response.end('fixture failure');return;}
 let result={id};
 if(request.method==='POST'){
  let body='';for await(const chunk of request)body+=chunk;
  result=JSON.parse(body);
  if(result.id==='rejected'){response.writeHead(400,{'content-type':'text/plain'});response.end('invalid item');return;}
 }
 response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify(result));
});
const client=new QueryClient({defaultOptions:{queries:{retry:false,gcTime:0},mutations:{retry:false,gcTime:0}}});
function run(command,args){const result=spawnSync(command,args,{cwd:temporary,env:process.env,encoding:'utf8',timeout:300000,maxBuffer:16*1024*1024});assert.equal(result.status,0,`${result.error??''}\n${result.stdout}\n${result.stderr}`);}
async function waitFor(check){const end=Date.now()+5000;while(!check()){assert.ok(Date.now()<end,'condition timed out');await new Promise(resolve=>setTimeout(resolve,10));}}
try{
 symlinkSync(join(dependencies,'node_modules'),join(temporary,'node_modules'));
 writeFileSync(join(temporary,'schema.json'),JSON.stringify({type:'object',required:['id'],properties:{id:{type:'string'}},additionalProperties:false}));
 run('cargo',['run','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','typescript_presence_client','--',join(temporary,'schema.json'),join(temporary,'sdk.ts')]);
 writeFileSync(join(temporary,'usage.ts'),`
import {useQuery,useMutation,queryOptions,mutationOptions} from '@tanstack/react-query';
import {createForkLaunchSdk,forkLaunchQueryOptions,forkLaunchMutationOptions} from './sdk';
const sdk=createForkLaunchSdk();
export function checkTypes(){
 const options=forkLaunchQueryOptions(['tenant','item','1'],sdk.presence.presence.readPresence,{params:{index:'1'}});
 const exactKey:readonly ['tenant','item','1']=options.queryKey;
 const query=useQuery({...queryOptions(options),select:result=>result.response.id});
 const data:string|undefined=query.data;
 // @ts-expect-error Select returns a string, not a number.
 const wrong:number|undefined=query.data;
 // @ts-expect-error Required route params remain required.
 forkLaunchQueryOptions(['missing'],sdk.presence.presence.readPresence,{});
 // @ts-expect-error Required route params retain their type.
 forkLaunchQueryOptions(['wrong'],sdk.presence.presence.readPresence,{params:{index:42}});
 const mutation=useMutation(mutationOptions(forkLaunchMutationOptions(['create'],sdk.presence.presence.writePresence)));
 mutation.mutate({body:{id:'created'}});
 // @ts-expect-error Mutation variables retain body schema.
 mutation.mutate({body:{id:42}});
 void [exactKey,data,wrong];
}
export async function statuses(){
 const result=await forkLaunchQueryOptions(['status'],sdk.presence.presence.readPresence,{params:{index:'1'}}).queryFn({signal:new AbortController().signal});
 const status:200=result.status;
 // @ts-expect-error Error statuses are thrown instead of returned as query data.
 const error:404=result.status;
 void [status,error];
}
`);
 writeFileSync(join(temporary,'tsconfig.json'),JSON.stringify({compilerOptions:{strict:true,target:'ES2022',module:'NodeNext',moduleResolution:'NodeNext',lib:['ES2022','DOM','DOM.Iterable'],types:[],outDir:'compiled'},include:['*.ts']}));
 run(join(dependencies,'node_modules/.bin/tsc'),['-p',join(temporary,'tsconfig.json')]);
 const {createForkLaunchSdk,forkLaunchQueryOptions:query,forkLaunchMutationOptions:mutation,ForkLaunchSdkHttpError}=require(join(temporary,'compiled/sdk.js'));
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const sdk=createForkLaunchSdk({baseUrl:`http://127.0.0.1:${server.address().port}`});
 const read=sdk.presence.presence.readPresence,write=sdk.presence.presence.writePresence;
 const options={...query(['tenant-a','item','one'],read,{params:{index:'one'}}),staleTime:Infinity,gcTime:Infinity};
 assert.equal((await client.fetchQuery(options)).response.id,'one');
 await client.fetchQuery(options);assert.equal(counts.get('one'),1);
 await client.invalidateQueries({queryKey:['tenant-a','item']});await client.fetchQuery(options);assert.equal(counts.get('one'),2);
 await client.fetchQuery(query(['tenant-b','item','one'],read,{params:{index:'one'}}));assert.equal(counts.get('one'),3);
 await assert.rejects(client.fetchQuery(query(['missing'],read,{params:{index:'missing'}})),error=>error instanceof ForkLaunchSdkHttpError&&error.result.status===404&&error.result.response==='fixture failure');
 const retried=await client.fetchQuery({...query(['retry'],read,{params:{index:'retry'}}),retry:1,retryDelay:0});assert.equal(retried.response.id,'retry');assert.equal(counts.get('retry'),2);
 const observer=new MutationObserver(client,mutation(['create'],write));
 assert.equal((await observer.mutate({body:{id:'created'}})).response.id,'created');
 await assert.rejects(observer.mutate({body:{id:'rejected'}}),error=>error instanceof ForkLaunchSdkHttpError&&error.result.status===400);
 const queryCaller=new AbortController();
 const canceled=client.fetchQuery(query(['slow-query'],read,{params:{index:'slow-query'},signal:queryCaller.signal}));const cancelCheck=assert.rejects(canceled,isCancelledError);
 await waitFor(()=>started.has('slow-query'));await client.cancelQueries({queryKey:['slow-query']});await cancelCheck;await waitFor(()=>closed.has('slow-query'));assert.equal(queryCaller.signal.aborted,false);
 const controller=new AbortController();
 const callerCanceled=client.fetchQuery(query(['slow-caller'],read,{params:{index:'slow-caller'},signal:controller.signal}));const callerCheck=assert.rejects(callerCanceled,error=>error.name==='AbortError');
 await waitFor(()=>started.has('slow-caller'));controller.abort();await callerCheck;await waitFor(()=>closed.has('slow-caller'));
 const alreadyAborted=new AbortController();alreadyAborted.abort();
 await assert.rejects(client.fetchQuery(query(['pre-aborted'],read,{params:{index:'pre-aborted'},signal:alreadyAborted.signal})),error=>error.name==='AbortError');assert.equal(counts.get('pre-aborted'),undefined);
 await assert.rejects(client.fetchQuery(query(['network'],read,{params:{index:'network'}})),error=>!(error instanceof ForkLaunchSdkHttpError));
 assert.equal((await mutation(['empty'],async()=>({status:204,response:undefined})).mutationFn({})).status,204);
 console.log('PASS actual TanStack Query/React type inference, cache scoping/invalidation, typed HTTP errors/retries, mutations, query/caller cancellation and network errors with generated Rust-service SDK');
}finally{
 client.clear();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));rmSync(temporary,{recursive:true,force:true});
}
