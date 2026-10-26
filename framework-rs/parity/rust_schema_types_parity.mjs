// Compile the actual shared model emitter's output with strict TypeScript.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtempSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const fixtures=[
 {schema:{$ref:"#/definitions/Base",properties:{tag:{enum:["a","b"]}},required:["tag"],definitions:{Base:{type:"object",properties:{id:{type:"integer"}},required:["id"]}}},valid:[{id:1,tag:"a"}],invalid:[{id:1},{tag:"a"},{id:1,tag:"c"}]},
 {schema:{const:{}},valid:[{}],invalid:[1,[],{extra:true}]},
 {schema:false,valid:[],invalid:[null,1,{},[]]},
 {schema:true,valid:[null,1,{},[]],invalid:[]},
 {schema:{const:{'special/~': ['a',1],enabled:true}},valid:[{'special/~':['a',1],enabled:true}],invalid:[{'special/~':['a',2],enabled:true},{'special/~':['a',1],enabled:false},{'special/~':['a',1]}]},
 {schema:{const:['a',{count:2},null]},valid:[['a',{count:2},null]],invalid:[['a',{count:3},null],['a',{count:2}],{}]},
 {schema:{enum:[{kind:'left',value:1},{kind:'right',value:'b'}]},valid:[{kind:'left',value:1},{kind:'right',value:'b'}],invalid:[{kind:'left',value:'b'},{kind:'other',value:1}]},
 {schema:{$schema:'http://json-schema.org/draft-07/schema#',type:'array',items:[{type:'string'},{type:'integer'}],additionalItems:false,minItems:1},valid:[['a'],['a',1]],invalid:[[],[1],['a','b'],['a',1,true]]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',type:'array',prefixItems:[{type:'string'},{type:'integer'}],items:{type:'boolean'},minItems:2},valid:[['a',1],['a',1,true,false]],invalid:[['a'],['a','b'],['a',1,'bad']]},
 {schema:{$schema:'https://json-schema.org/draft/2020-12/schema',type:'array',prefixItems:[{type:'string'},{type:'integer'}],items:false,maxItems:1},valid:[[],['a']],invalid:[['a',1],[1]]},
 {schema:{allOf:[{type:'object',required:['left'],properties:{left:{type:'string'}}},{type:'object',required:['right'],properties:{right:{type:'integer'}}}]},valid:[{left:'a',right:1}],invalid:[{left:'a'},{right:1},{left:1,right:1}]},
 {schema:{type:'object',required:['id'],properties:{id:{type:'integer'}},allOf:[{properties:{tag:{enum:['a','b']}},required:['tag']}]},valid:[{id:1,tag:'a'}],invalid:[{id:1},{id:'x',tag:'a'},{id:1,tag:'c'}]},
 {schema:{anyOf:[{allOf:[{type:'string'},{enum:['a','b']}]},{type:'integer'}]},valid:['a','b',1],invalid:['c',true]},
];
const inputs=fixtures.map(({schema,valid,invalid})=>({schema,cases:[...valid.map(value=>[value,true]),...invalid.map(value=>[value,false])]}));
const outputs=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','build,asyncapi-import','--example','codegen_schema_bundle'],{cwd:root,input:JSON.stringify(inputs),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},timeout:300000}));
const temporary=mkdtempSync(path.join(tmpdir(),'fl-schema-types-'));
try {
 let source='';
 let accepted=0,rejected=0;
 for(const [index, fixture] of fixtures.entries()) {
  const output=outputs[index];
  const expected=inputs[index].cases.map(entry=>entry[1]);
  assert.deepEqual(output.original,expected,`native input ${index}`);
  assert.deepEqual(output.compiled,expected,`native bundle ${index}`);
  source+=`namespace Fixture${index} {\n`;
  for(const [name,type] of Object.entries(output.definitions)) source+=`type ${name} = ${type};\n`;
  source+=`type Model = ${output.typescript};\n`;
  for(const [valueIndex,value] of fixture.valid.entries()) {source+=`export const valid${valueIndex}: Model = ${JSON.stringify(value)};\n`;accepted++;}
  for(const [valueIndex,value] of fixture.invalid.entries()) {source+=`// @ts-expect-error Invalid schema value must fail static checking.\nexport const invalid${valueIndex}: Model = ${JSON.stringify(value)};\n`;rejected++;}
  source+='}\n';
 }
 const file=path.join(temporary,'models.ts');writeFileSync(file,source);
 execFileSync(process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc'),['--strict','--noEmit','--skipLibCheck','--target','ES2022',file],{cwd:temporary,stdio:'inherit',timeout:300000});
 console.log(`PASS ${fixtures.length} shared schema model fixtures: ${accepted} accepted and ${rejected} rejected strict TypeScript values, matching original/bundled native validation`);
} finally {rmSync(temporary,{recursive:true,force:true});}
