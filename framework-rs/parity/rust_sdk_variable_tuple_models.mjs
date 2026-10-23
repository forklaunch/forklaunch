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
const {z}=require('zod/v3'),Ajv=require('ajv'),Ajv2020=require('ajv/dist/2020').default;
const env={...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:process.env.CARGO_BUILD_JOBS??'2'};
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'));
function run(command,args,options={}){const result=spawnSync(command,args,{cwd:root,env,encoding:'utf8',maxBuffer:64*1024*1024,...options});assert.equal(result.status,0,`${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);return result.stdout;}
const dialect='https://json-schema.org/draft/2020-12/schema';
const values=[null,{},'',[],[null],['x'],[true],['x',true],[null,false],['x',null],['x',true,1],['x',true,1,2],['x',true,1,2,3],['x',true,1,2,3,4],['x',true,'tail'],['x',true,null],['x',true,{}],['x',true,[]],[null,null]];
const fixtures=[];
for(const modern of [false,true])for(const minimum of [0,1,2,3])for(const maximum of [3,5])for(const tail of [false,true,{type:'integer'}]){
 if(tail===false&&minimum>2)continue;
 const prefix=[{type:['string','null']},{type:'boolean'}];
 const schema={type:'array',minItems:minimum,maxItems:maximum,...(modern?{$schema:dialect,prefixItems:prefix,items:tail}:{items:prefix,additionalItems:tail})};
 const validate=new (modern?Ajv2020:Ajv)({strict:false}).compile(schema);
 fixtures.push({schema,cases:values.map(value=>({value,valid:validate(value)}))});
}
const zod=z.tuple([z.string().nullable(),z.boolean()]).rest(z.number().int());
fixtures.push({schema:validator.openapi2020(zod),cases:values.map(value=>({value,valid:validator.parse(zod,value).ok}))});
const constructionIndex=fixtures.length;
fixtures.push({schema:{$schema:dialect,type:'array',prefixItems:[{type:['string','null']},{type:['boolean','null']}],items:{type:'string'},maxItems:4},cases:[{value:[],valid:true},{value:[null],valid:true},{value:[null,null],valid:true},{value:[null,true,'tail'],valid:true}]});
for(const schema of [
 {$schema:dialect,type:'array',prefixItems:[{$ref:'#'}],items:false},
 {$schema:dialect,type:'array',prefixItems:[{type:'string'}],items:{$ref:'#'},minItems:1},
 {$schema:dialect,$defs:{Node:{type:'array',prefixItems:[{$ref:'#/$defs/Node'}],items:false}},$ref:'#/$defs/Node'},
]){
 const validate=new Ajv2020({strict:false}).compile(schema);
 const inputs=[[],[[]],[[[]]],['x'],['x',['y']],['x',null],[null]];
 fixtures.push({schema,cases:inputs.map(value=>({value,valid:validate(value)}))});
}
for(const tail of [{type:['string','null']},{anyOf:[{type:'string'},{type:'boolean'}]}]){
 const schema={$schema:dialect,type:'array',prefixItems:[{type:'string'}],items:tail,minItems:1};
 const validate=new Ajv2020({strict:false}).compile(schema);
 fixtures.push({schema,cases:[[],['x'],['x',null],['x',true],['x','y'],['x',42]].map(value=>({value,valid:validate(value)}))});
}
run('cargo',['build','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-core','--features','build,asyncapi-import','--example','codegen_fixed_tuple_models']);
const generated=JSON.parse(run(path.join(target,'debug/examples/codegen_fixed_tuple_models'),[],{input:JSON.stringify(fixtures.map(entry=>entry.schema))}));
// Root references exercise both the local visitor type and generic parameter scopes.
for(const name of ['SequenceVisitor','A','D','S']) {
 const schema={$schema:dialect,type:'array',prefixItems:[{$ref:'#'}],items:{$ref:'#'},maxItems:3};
 const inputs=[[],[[]],[[[]]],[[],[]],['x'],[null],[[],[],[],[]]];
 const validate=new Ajv2020({strict:false}).compile(schema);
 fixtures.push({schema,cases:inputs.map(value=>({value,valid:validate(value)}))});
 const [named]=JSON.parse(run(path.join(target,'debug/examples/codegen_fixed_tuple_models'),[name],{input:JSON.stringify([schema])}));
 assert.equal(named.root,name);generated.push(named);
}
assert.equal(generated.length,fixtures.length);
for(const item of generated)assert.ok(item.sequences>0);
const work=mkdtempSync(path.join(tmpdir(),'fl-variable-tuple-models-'));
try{
 mkdirSync(path.join(work,'src'));
 writeFileSync(path.join(work,'Cargo.toml'),'[package]\nname="fl-variable-tuple-conformance"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nserde={version="1",features=["derive"]}\nserde_json="=1.0.151"\n');
 let count=0;
 const source=generated.map((item,index)=>{
  const checks=fixtures[index].cases.map(({value,valid})=>{count++;return `{let wire=${JSON.stringify(JSON.stringify(value))};let decoded=serde_json::from_str::<${item.root}>(wire);assert_eq!(decoded.is_ok(),${valid},"case ${index}: {wire}");${valid?'assert_eq!(serde_json::to_value(decoded.unwrap()).unwrap(),serde_json::from_str::<serde_json::Value>(wire).unwrap());':''}}`;}).join('\n');
  const constructed=index!==constructionIndex?'':`
   #[test] fn invalid_construction_does_not_write() {
    for value in [TupleCase{item0:None,item1:Some(Some(true)),rest:vec![]},TupleCase{item0:None,item1:None,rest:vec!["x".into()]},TupleCase{item0:Some(None),item1:Some(None),rest:vec!["a".into(),"b".into(),"c".into()]}] {
     let mut bytes=Vec::new();assert!(serde_json::to_writer(&mut bytes,&value).is_err());assert!(bytes.is_empty());
    }
    let value=TupleCase{item0:Some(None),item1:None,rest:vec![]};assert_eq!(serde_json::to_string(&value).unwrap(),"[null]");
   }`;
  return `pub mod case${index} {${item.source}\n#[test]fn conformance(){${checks}}${constructed}}`;
 }).join('\n');
 writeFileSync(path.join(work,'src/lib.rs'),source);
 // Offline builds need every resolved package, including other targets' dependencies a host build never downloads; fetch them first.
 run('cargo',['fetch','--manifest-path',path.join(work,'Cargo.toml')]);
 run('cargo',['rustc','--offline','--manifest-path',path.join(work,'Cargo.toml'),'--lib','--','-D','warnings']);
 run('cargo',['test','--offline','--manifest-path',path.join(work,'Cargo.toml')]);
 const types=generated.map((item,index)=>`namespace Case${index} {${Object.entries(item.definitions).map(([name,type])=>`export type ${name}=${type};`).join('\n')}export type Value=${item.typescript};${fixtures[index].cases.filter(item=>item.valid).map((item,i)=>`const valid${i}:Value=${JSON.stringify(item.value)};`).join('\n')}}`).join('\n');
 writeFileSync(path.join(work,'types.ts'),types);
 run(process.env.TSC_BIN??'tsc',['--noEmit','--strict','--skipLibCheck','--target','es2022',path.join(work,'types.ts')]);
 console.log(`PASS ${count} generated Rust acceptance/JSON roundtrips (${values.length} actual Zod, remaining real AJV7/2020) across ${fixtures.length} typed prefix/rest schemas; TS assignments, missing/null, closed/open/typed tails, min/max, recursive aliases and pre-write construction rejection`);
}finally{rmSync(work,{recursive:true,force:true});}
