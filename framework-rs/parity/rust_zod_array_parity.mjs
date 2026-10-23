import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const values=[undefined,null,false,true,0,1,'', 'array', {},{length:1},[],[null],[1],[' raw '],[{},['nested']], [null,true,3,'x',{},[]]];
for(let count=2;count<=12;count++)values.push(Array.from({length:count},(_,i)=>i));
const operations=[['nonempty'],...['min','max','length'].flatMap(kind=>[0,1,2,3,4294967295].map(limit=>[kind,limit]))];
const chains=[[],...operations.map(step=>[step]),...operations.flatMap(first=>operations.map(second=>[first,second])),...operations.flatMap(first=>operations.flatMap(second=>operations.map(third=>[first,second,third])))];
const cases=[],expected=[];
for(const steps of chains){
 let schema=statics.array(statics.unknown);
 for(const [kind,limit]of steps)schema=schema[kind](...(limit===undefined?[]:[limit]));
 for(const value of values){
  cases.push({steps,...(value===undefined?{}:{value})});
  expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));
 }
}
if(process.env.FORKLAUNCH_ARRAY_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod v3 array-container observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_array_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod v3/native array-container comparisons: last-check replacement, fixed diagnostic order, strict type/length and untouched elements`);
}
