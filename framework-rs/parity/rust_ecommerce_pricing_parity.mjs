import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';
import http from 'node:http';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
async function load(relative){
 const source=await readFile(path.join(root,relative),'utf8');const exports={};
 vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require(spec){throw Error('unexpected runtime import '+spec);}});return exports;
}
const {StripeTaxService}=await load('blueprint/ecommerce-stripe/domain/services/tax.service.ts');
const {FlatRateShippingService}=await load('blueprint/ecommerce-stripe/domain/services/shipping.service.ts');
const address={name:'Buyer',line1:'Private Street',city:'Oakland',state:'CA',postalCode:'94601',country:'US'};
const cases=[];
for(const country of ['US','us','CA',''])for(const subtotalCents of [0,4999,5000,9000])cases.push({operation:'shipping',request:{shippingAddress:{...address,country},subtotalCents}});
const response={tax_amount_exclusive:123,tax_breakdown:[{amount:100,tax_rate_details:{state:'CA',country:'US'}},{amount:20,tax_rate_details:{country:'US'}},{amount:3,tax_rate_details:{state:'',country:''}}]};
for(const scenario of ['success','empty','failure'])for(const lineItemCents of [[],[0],[50],[150],[100,200,415]])cases.push({operation:'tax',scenario,request:{shippingAddress:address,lineItemCents}});
cases.push({operation:'tax',scenario:'success',request:{shippingAddress:{...address,line2:'Apt 💬'},lineItemCents:[100]}});
const calls=[],warnings=[],expected=[];
for(const testcase of cases){
 let value;
 if(testcase.operation==='shipping')value=await new FlatRateShippingService().calculate(testcase.request);
 else value=await new StripeTaxService({tax:{calculations:{async create(params){calls.push({scenario:testcase.scenario,params});if(testcase.scenario==='failure')throw Error('fixture provider down');return testcase.scenario==='empty'?{tax_amount_exclusive:0}:structuredClone(response);}}}},{warn(...args){warnings.push(args);}}).calculate(testcase.request);
 expected.push({ok:JSON.parse(JSON.stringify(value))});
}
assert.equal(warnings.length,5);
const wire=[];
const provider=http.createServer(async(req,res)=>{
 let raw='';for await(const chunk of req)raw+=chunk;wire.push({path:req.url,headers:req.headers,body:new URLSearchParams(raw)});
 res.setHeader('content-type','application/json');
 if(req.url.startsWith('/failure/')){res.writeHead(503);res.end(JSON.stringify({error:'provider-private-details'}));}
 else if(req.url.startsWith('/malformed/'))res.end('not-json');
 else if(req.url.startsWith('/negative/'))res.end(JSON.stringify({tax_amount_exclusive:-1}));
 else if(req.url.startsWith('/large/'))res.end('x'.repeat(4097));
 else if(req.url.startsWith('/redirect/')){res.writeHead(302,{location:'/must-not-follow'});res.end('{}');}
 else if(req.url.startsWith('/empty/'))res.end(JSON.stringify({tax_amount_exclusive:0}));
 else res.end(JSON.stringify(response));
});
await new Promise(resolve=>provider.listen(0,'127.0.0.1',resolve));
try{
 const nativeFallbacks=['malformed','negative','large','redirect'].map(scenario=>({operation:'tax',scenario,request:{shippingAddress:address,lineItemCents:[100]}}));
 const nativeCases=[...cases,...nativeFallbacks,{operation:'tax',scenario:'failure',request:{shippingAddress:address,lineItemCents:[-1]}},{operation:'tax',scenario:'failure',request:{shippingAddress:address,lineItemCents:[9223372036854775807n,1]}}];
 // Send i64 boundary values exactly rather than rounding them through JavaScript numbers.
 const input=JSON.stringify({base:`http://127.0.0.1:${provider.address().port}`,cases:nativeCases},(_key,value)=>typeof value==='bigint'?'I64_MAX':value).replace('"I64_MAX"','9223372036854775807');
 const child=spawn('cargo',['run','--quiet','--manifest-path','blueprint-rs/Cargo.toml','-p','forklaunch-ecommerce-stripe','--example','pricing_parity'],{cwd:root,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',data=>output+=data);child.stdin.end(input);
 assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);
 const actual=JSON.parse(output);assert.deepEqual(actual.slice(0,cases.length),expected);assert.deepEqual(actual.slice(cases.length,cases.length+nativeFallbacks.length),nativeFallbacks.map(()=>({ok:{taxCents:7,estimated:true,breakdown:[{jurisdiction:'estimated-fallback',taxCents:7}]}})));
 assert.deepEqual(actual.slice(cases.length+nativeFallbacks.length),[{error:'ecommerce_validation'},{error:'ecommerce_validation'}]);
 assert.equal(wire.length,calls.length+nativeFallbacks.length);
 for(let i=0;i<calls.length;i++){
  const {params,scenario}=calls[i],request=wire[i];assert.equal(request.path,`/${scenario}/v1/tax/calculations`);assert.equal(request.headers.authorization,'Bearer sk_fixture');
  const expectedFields={currency:'usd','customer_details[address_source]':'shipping'};
  for(let index=0;index<params.line_items.length;index++)for(const [key,value]of Object.entries(params.line_items[index]))expectedFields[`line_items[${index}][${key}]`]=String(value);
  for(const [key,value]of Object.entries(params.customer_details.address))if(value!==undefined)expectedFields[`customer_details[address][${key}]`]=value;
  assert.deepEqual(Object.fromEntries(request.body),expectedFields);
 }
 console.log(`PASS ${cases.length} actual TypeScript/Rust tax/shipping cases: Stripe wire fields, jurisdiction lines, explicit estimated fallback, cent rounding, rate thresholds and native overflow rejection`);
}finally{await new Promise(resolve=>provider.close(resolve));}
