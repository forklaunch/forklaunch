import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const values=[undefined,null,false,true,0,1,-1,0.5,-0.5,12.99,[],[null],['2'],[true],{}, {toString:'blocked'},'', 'NaN', 'Infinity','-Infinity','-0','0.1','12.99','9007199254740991','9007199254740992','9007199254740993','-9007199254740991','-9007199254740992','1e300','1e309','1e-324','5e-324','0x20000000000001',' 2 ','\u00852\u0085','\ufeff2\ufeff'];
for(const bound of [-9007199254740991,-2147483648,-2,0,2,2147483647,9007199254740991])for(const delta of [-2,-1,-.5,0,.5,1,2])values.push(String(bound+delta));
const operations=[['int'],['finite'],['safe'],...['min','max','gt','lt'].flatMap(kind=>[-9007199254740991,-2,0,2,9007199254740991].map(limit=>[kind,limit]))];
const chains=[[],...operations.map(step=>[step]),...operations.flatMap(first=>operations.map(second=>[first,second])),[['int'],['finite'],['safe']]];
const cases=[],expected=[];
for(const steps of chains){
 let constraints=statics.number.innerType();
 for(const [kind,limit] of steps)constraints=constraints[kind](...(limit===undefined?[]:[limit]));
 // Factory is ZodEffects: pipe preserves its actual Number(value) preprocessing.
 const schema=statics.number.pipe(constraints);
 for(const value of values){
  cases.push({steps,...(value===undefined?{}:{value})});
  const result=validator.parse(schema,value);
  if(result.ok){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(result.value);expected.push({ok:true,bits:bytes.toString('hex')});}
  else expected.push(JSON.parse(JSON.stringify(result)));
 }
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod v3 ordered number observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_number_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod v3/native ordered number comparisons: complete diagnostics and IEEE-754 output bits, including signed zero and infinity`);
}
