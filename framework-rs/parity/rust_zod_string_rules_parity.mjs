import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const texts=[undefined,null,false,1,[],{},'','a','ab',' ba ','💡a','a💡','é','\u0085a','\ufeffa','"x"','a\nb'];
const needles=['','a','b','💡','́','"','\n'];
const positions=[undefined,-Infinity,-3,-0,0,.5,1,1.5,2,3,100,Infinity,NaN];
const cases=[],expected=[];
for(const text of texts)for(const needle of needles)for(const method of ['startsWith','endsWith','includes'])for(const position of method==='includes'?positions:[undefined])for(const order of ['plain','trimFirst','trimLast','mixed']){
 let schema=statics.string;if(order==='trimFirst')schema=schema.trim();schema=method==='includes'?schema.includes(needle,{position}):schema[method](needle);
 const rules=[];if(order==='trimFirst')rules.push(['trim']);
 rules.push(position===undefined?[method,needle]:[method,needle,Object.is(position,-0)?'-0':Number.isFinite(position)?position:String(position)]);
 if(order==='trimLast'){schema=schema.trim();rules.push(['trim']);}
 if(order==='mixed'){schema=schema.min(5).trim().endsWith('x');rules.push(['min',5],['trim'],['endsWith','x']);}
 const result=validator.parse(schema,text);
 cases.push({rules,...(text===undefined?{}:{value:text})});expected.push(JSON.parse(JSON.stringify(result)));

}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod string literal observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_string_rules_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod/native string literal comparisons with complete diagnostics and transformed values`);
}
