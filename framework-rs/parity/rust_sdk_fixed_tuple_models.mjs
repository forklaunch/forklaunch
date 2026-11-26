import assert from 'node:assert/strict';
import {mkdtempSync,writeFileSync,mkdirSync,rmSync} from 'node:fs';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator}=actualValidator(root);
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3');
const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:process.env.CARGO_BUILD_JOBS??'2'};
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'));
function run(command,args,options={}){const result=spawnSync(command,args,{cwd:root,env,encoding:'utf8',maxBuffer:32*1024*1024,...options});assert.equal(result.status,0,`${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);return result.stdout;}
const fixtures=[];
for(const size of [0,1,2,16,17,32,128]){
 const schema=z.tuple(Array.from({length:size},(_,i)=>i%2?z.boolean():z.string())),valid=Array.from({length:size},(_,i)=>i%2?true:'x');
 const inputs=[valid,valid.slice(1),[...valid,'extra'],null,{},'text'];
 if(size)inputs.push(valid.map((v,i)=>i===0?null:v));
 fixtures.push({schema:validator.openapi2020(schema),cases:inputs.map(value=>({value,valid:validator.parse(schema,value).ok}))});
}
for(const schema of [z.tuple([z.string().nullable(),z.tuple([z.boolean()])]),z.object({tuple:z.tuple([z.string(),z.boolean()])}),z.array(z.tuple([z.string()]))]){
 const inputs=[[null,[true]],['x',[false]],{tuple:['x',true]},[['x']],[],[null,[]],{tuple:['x']},[['x','extra']]];
 fixtures.push({schema:validator.openapi2020(schema),cases:inputs.map(value=>({value,valid:validator.parse(schema,value).ok}))});
}
const zodCases=fixtures.reduce((count,entry)=>count+entry.cases.length,0);
const legacy={type:'array',items:[{type:'string'},{type:'boolean'}],minItems:2,additionalItems:false};
fixtures.push({schema:legacy,cases:[{value:['x',true],valid:true},{value:['x'],valid:false},{value:['x',true,1],valid:false}]});
fixtures.push({schema:{type:'array',prefixItems:[{type:'string'},{type:'boolean'}],minItems:1,maxItems:1,$schema:'https://json-schema.org/draft/2020-12/schema'},cases:[{value:['x'],valid:true},{value:['x',true],valid:false}]});
const recursive={$schema:'https://json-schema.org/draft/2020-12/schema',$defs:{Node:{type:'array',prefixItems:[{$ref:'#/$defs/Node'}],minItems:1,items:false}},$ref:'#/$defs/Node'};
fixtures.push({schema:recursive,cases:[]});
fixtures.push({schema:{type:'array',prefixItems:[{type:'number'}],items:{type:'string'},minItems:1,maxItems:1},cases:[{value:['x'],valid:true},{value:[1],valid:false}],broad:true});
const schemas=fixtures.map(entry=>entry.schema);
run('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','build,asyncapi-import','--example','codegen_fixed_tuple_models']);
const generated=JSON.parse(run(path.join(target,'debug/examples/codegen_fixed_tuple_models'),[],{input:JSON.stringify(schemas)}));
assert.equal(generated.length,fixtures.length);
for(const [index,item] of generated.entries())if(!fixtures[index].broad)assert.ok(item.sequences>0);
const work=mkdtempSync(path.join(tmpdir(),'fl-fixed-tuple-models-'));
try{
 mkdirSync(path.join(work,'src'));
 writeFileSync(path.join(work,'Cargo.toml'),'[package]\nname="fl-fixed-tuple-conformance"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\n');
 let count=0;
 const source=generated.map((item,index)=>{
  const checks=fixtures[index].cases.map(({value,valid})=>{count++;const json=JSON.stringify(value);return `{let wire=${JSON.stringify(json)};let decoded=serde_json::from_str::<${item.root}>(wire);assert_eq!(decoded.is_ok(),${valid},"case ${index}: {wire}");${valid?'assert_eq!(serde_json::to_value(decoded.unwrap()).unwrap(),serde_json::from_str::<serde_json::Value>(wire).unwrap());':''}}`;}).join('\n');
  return `mod case${index} {${item.source}\n#[test]fn conformance(){${checks}}}`;
 }).join('\n');
 writeFileSync(path.join(work,'src/lib.rs'),source);
 // Offline builds need every resolved package, including other targets' dependencies a host build never downloads; fetch them first.
 run('cargo',['fetch','--manifest-path',path.join(work,'Cargo.toml')]);
 run('cargo',['test','--offline','--manifest-path',path.join(work,'Cargo.toml')]);
 const types=generated.map((item,index)=>`namespace Case${index} {${Object.entries(item.definitions).map(([name,type])=>`export type ${name}=${type};`).join('\n')}export type Value=${item.typescript};${fixtures[index].cases.filter(item=>item.valid).map((item,i)=>`const valid${i}:Value=${JSON.stringify(item.value)};`).join("\n")}}`).join('\n');
 writeFileSync(path.join(work,'types.ts'),types);
 run(process.env.TSC_BIN??'tsc',['--noEmit','--strict','--skipLibCheck','--target','es2022',path.join(work,'types.ts')]);
 console.log(`PASS ${count} generated Rust acceptance and exact JSON roundtrip cases (${zodCases} actual Zod, ${count-zodCases} schema/dialect controls) across ${fixtures.length} schemas; compiled TS models, arity0/1/2/16/17/32/128, nested/ref/nullable/legacy/truncated prefixes and required recursion`);
}finally{rmSync(work,{recursive:true,force:true});}
