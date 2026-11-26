import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {actualHttpSources} from './rust_actual_http_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualHttpSources(root);
const {createModelGatewayClient}=load(path.join(root,'framework/core/src/http/modelGatewayClient.ts'));
const requests=[];
const responses=[];
const client=createModelGatewayClient({gatewayUrl:'https://gateway.example.test',instanceId:'dto-fixture',hmacKey:'dto-fixture-key',fetch:async(url,options)=>{
 if(options.body)requests.push(JSON.parse(options.body));
 return new Response(JSON.stringify(responses.shift()),{headers:{'content-type':'application/json'}});
}});
const usage={prompt_tokens:9007199254740993,completion_tokens:1.5,total_tokens:9007199254740994,extraUsage:true};
const completion={id:'c',object:'chat.completion',created:123.25,model:'m',choices:[{index:0,message:{role:'assistant',content:null,tool_calls:[{id:'t'}]},finish_reason:null,extraChoice:[null]}],usage,extraCompletion:{nested:true}};
const catalog={models:[{id:'m',baa:true,extraModel:'retained'}],monthlyTokenBudget:100,tokensUsedThisMonth:2,requestsPerMinute:3,withheldForHipaa:['other'],extraCatalog:null};
const chunks=[{id:'c',object:'chat.completion.chunk',model:'m',choices:[{index:0,delta:{},extraChoice:true}]},...[null,'hello'].map(content=>({id:'c',object:'chat.completion.chunk',model:'m',choices:[{index:0,delta:{role:'assistant',content,extraDelta:[]},finish_reason:null}],usage,extraChunk:true}))];
const actions=[];
for(const role of ['system','developer','user','assistant','tool']){
 const request={model:'m',messages:[{role,content:null,extraMessage:{data:'💬'}}],extraRequest:true,temperature:undefined,stop:undefined};
 responses.push(completion);const result=await client.chat.completions.create(request);actions.push({shape:'request',body:requests.at(-1)},{shape:'completion',body:result});
}
for(const content of ['text',[{type:'text',text:'hello'}],{arbitrary:true}]){
 const request={model:'m',messages:[{role:'user',content}],temperature:.5,top_p:1,max_tokens:10,max_completion_tokens:20,response_format:null,tools:[{type:'function'}],tool_choice:null,parallel_tool_calls:false,stop:null,seed:9007199254740993,presence_penalty:-.5,frequency_penalty:0,reasoning_effort:'high'};
 responses.push(completion);await client.chat.completions.create(request);actions.push({shape:'request',body:requests.at(-1)});
}
responses.push(catalog);actions.push({shape:'models',body:await client.models()});
for(const chunk of chunks){
 const streaming=createModelGatewayClient({gatewayUrl:'https://gateway.example.test',instanceId:'dto-fixture',hmacKey:'dto-fixture-key',fetch:async()=>new Response(`data: ${JSON.stringify(chunk)}\n\ndata: [DONE]\n\n`)});
 for await(const body of streaming.chat.completions.stream({model:'m',messages:[]}))actions.push({shape:'chunk',body});
}
actions.push({shape:'models',body:{models:[],monthlyTokenBudget:0,tokensUsedThisMonth:0,requestsPerMinute:0}},{shape:'completion',body:{...completion,usage:undefined}});
if(process.env.FORKLAUNCH_MODEL_DTOS_TS_ONLY==='1')console.log(`PASS ${actions.length} actual TypeScript managed DTO JSON observations`);
else{
 execFileSync('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','instance-gateway','--example','model_gateway'],{cwd:root,stdio:'inherit'});
 const binary=path.join(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'),'debug/examples/model_gateway');
 const actual=JSON.parse(execFileSync(binary,[],{input:JSON.stringify({base:'http://127.0.0.1:1',actions:actions.map(action=>({kind:'dto',...action}))}),encoding:'utf8'}));
 assert.deepEqual(actual,JSON.parse(JSON.stringify(actions.map(action=>action.body))));
 console.log(`PASS ${actions.length} actual TypeScript/native typed DTO roundtrips: requests, completion required created, catalog, chunk absent/null, extras and finite number semantics`);
}
