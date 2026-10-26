import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const bases=['a@b.co',"a'b@b-.co",'a_b+z@abc.technology','00000000-0000-0000-0000-000000000000','ffffffff-ffff-ffff-ffff-ffffffffffff'];
const chars=['a','Z','0','-','_',"'",'.','@','%','+','\n','\r','\u2028','\u2029','é','K','ſ','💡','\0',' '];
const values=[undefined,null,false,1,[],{},'',...bases,...bases.map(value=>' '+value+' '),...bases.map(value=>value+'\r\n')];
for(const base of bases)for(let pos=0;pos<=base.length;pos++)for(const ch of chars)values.push(base.slice(0,pos)+ch+base.slice(pos),base.slice(0,pos)+ch+base.slice(pos+1));
const cases=[],expected=[];
for(const format of ['uuid','email'])for(const order of ['plain','trimFirst','mixed'])for(const message of [undefined,'','custom\n💡\0']){
 let schema=statics.string;if(order==='trimFirst')schema=schema.trim();schema=schema[format](message===undefined?undefined:{message});if(order==='mixed')schema=schema.trim().endsWith('z');
 for(const value of values){
  cases.push({format,order,...(message===undefined?{}:{message}),...(value===undefined?{}:{value})});
  expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));
 }
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch string format observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_string_formats_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod/native UUID/email format comparisons: ordered diagnostics, literal messages and transformed values`);
}
