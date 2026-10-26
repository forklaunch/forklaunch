import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const {z}=require('zod/v3');
const inputs=[undefined,'null','true','false','0','-0','1','-1','9007199254740993','1.2345678901234567','1e308','5e-324','{}','{"toString":null}','{"valueOf":0}','{"toString":{},"valueOf":null}'];
const strings=['','0','-0','+0','.5','1.','+1e2','1e-2','1e309','-1e309','1e-4000','-1e-4000','Infinity','+Infinity','-Infinity','inf','+inf','infinity','NaN','nan','true','TRUE','TrUe','false','FALSE',' false ','1','0','0x','0b2','0o8','0xff','+0xff','-0xff','0b101','0o777','01','1_000','1n','١','１２','1e','1e+','.', '+', '--1','0x1.1','0x1p2','0xＦＦ'];
for(const whitespace of ['\t','\n','\v','\f','\r',' ','\u00a0','\u1680','\u2000','\u2001','\u2002','\u2003','\u2004','\u2005','\u2006','\u2007','\u2008','\u2009','\u200a','\u2028','\u2029','\u202f','\u205f','\u3000','\ufeff','\u0085','\u180e','\u200b','\u0000']) strings.push(whitespace,`${whitespace}1`, `1${whitespace}`, `${whitespace}true`, `false${whitespace}`);
for(const value of strings)inputs.push(JSON.stringify(value));
for(const raw of [...inputs].filter(value=>value!==undefined)) inputs.push(`[${raw}]`,`[[${raw}]]`,`[null,${raw}]`,`[${raw},null]`);
inputs.push('[]','[[]]','[[[]]]','[null]','[[null]]','[[],[]]','[1,2]','[[1,2]]','[[[true]]]');
for(const bits of [1n,2n,51n,52n,53n,54n,55n,63n,64n,127n,128n,511n,1022n,1023n,1024n,1025n]) {
 for(const offset of [-3n,-2n,-1n,0n,1n,2n,3n,7n,15n,31n]){
  const integer=(1n<<bits)+offset;if(integer<0n)continue;
  for(const [radix,prefix] of [[2,'0b'],[8,'0o'],[16,'0x']])inputs.push(JSON.stringify(prefix+integer.toString(radix)));
 }
}
// Include long literals with low sticky bits after an exact halfway prefix.
let seed=0x31415926n;
for(let i=0;i<300;i++){
 let value=1n;const digits=20+(i%245);
 for(let j=0;j<digits;j++){seed=(seed*1664525n+1013904223n)&0xffffffffn;value=(value<<4n)|(seed&15n);}
 inputs.push(JSON.stringify('0x'+value.toString(16)));
}
for(const bits of [54n,64n,128n,512n,1024n]){
 const half=1n<<(bits-54n);
 for(const top of [(1n<<52n),(1n<<52n)+1n,(1n<<53n)-1n]){
  for(const offset of [-1n,0n,1n]){
   const value=(top<<(bits-53n))+half+offset;
   for(const [radix,prefix] of [[2,'0b'],[8,'0o'],[16,'0x']]) inputs.push(JSON.stringify(prefix+value.toString(radix)));
  }
 }
}
// JSON numeric values must retain the same binary number as JSON.parse.
let numericSeed=0x123456789abcdef0n;
for(let i=0;i<200;i++){
 numericSeed=(numericSeed*6364136223846793005n+1442695040888963407n)&((1n<<64n)-1n);
 const view=new DataView(new ArrayBuffer(8));view.setBigUint64(0,numericSeed);
 const value=view.getFloat64(0);if(Number.isFinite(value))inputs.push(JSON.stringify(value));
}
const cases=inputs.flatMap(raw=>['number','integer','boolean'].map(kind=>({kind,raw})));
function expected({kind,raw}){
 const input=raw===undefined?undefined:JSON.parse(raw);
 const schema=kind==='number'?statics.number:kind==='boolean'?statics.boolean:statics.number.pipe(z.number().int());
 const result=validator.parse(schema,input);
 if(!result.ok)return JSON.parse(JSON.stringify(result));
 if(kind==='boolean')return {ok:true,value:result.value};
 const buffer=new ArrayBuffer(8), view=new DataView(buffer);view.setFloat64(0,result.value);
 return {ok:true,bits:view.getBigUint64(0).toString(16).padStart(16,'0')};
}
const oracle=cases.map(expected);
if(process.env.FORKLAUNCH_SCALAR_ORACLE_ONLY==='1'){
 console.log(`PASS actual source oracle generated ${cases.length} scalar outcomes`);
}else{
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','typescript_coerce_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:32*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let i=0;i<cases.length;i++)assert.deepEqual(results[i],oracle[i],JSON.stringify(cases[i]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod scalar outcomes, IEEE-754 bits, radix ties/overflow, whitespace and nested JSON values`);
}
