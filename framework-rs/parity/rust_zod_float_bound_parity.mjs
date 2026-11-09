import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const bounds=[0,-0,.1,-.1,.3,Number.MIN_VALUE,-Number.MIN_VALUE,1e-7,1e-6,1e21,1e100,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,Number.MAX_VALUE,NaN,Infinity,-Infinity];
const values=[undefined,null,false,true,{},[],[null],['.3'],'NaN','Infinity','-Infinity','-0','',0,.1,-.1,.3,.1+.2,Number.MIN_VALUE,-Number.MIN_VALUE,1e-7,1e21,Number.MAX_VALUE];
const bytes=Buffer.alloc(8);
for(const value of bounds){
 if(!Number.isFinite(value)||value===0)continue;
 bytes.writeDoubleBE(value);const bits=bytes.readBigUInt64BE();
 for(const delta of [-1n,0n,1n]){bytes.writeBigUInt64BE(bits+delta);const adjacent=bytes.readDoubleBE();if(Number.isFinite(adjacent))values.push(adjacent);}
}
let seed=124n;const mask=(1n<<64n)-1n;
for(let i=0;i<128;i++){seed=(seed*6364136223846793005n+1442695040888963407n)&mask;bytes.writeBigUInt64BE(seed);const value=bytes.readDoubleBE();if(Number.isFinite(value))values.push(value);}
function encode(value){return Object.is(value,-0)?'-0':typeof value==='number'&&!Number.isFinite(value)?String(value):value;}
const cases=[],expected=[];
for(const bound of bounds)for(const method of ['min','max','gt','lt'])for(const order of ['single','intFirst','intLast','finiteFirst','contradictory']){
 const steps=[];
 if(order==='intFirst')steps.push(['int']);
 if(order==='finiteFirst')steps.push(['finite']);
 steps.push([method,bound]);
 if(order==='intLast')steps.push(['int']);
 if(order==='contradictory')steps.push([method==='min'||method==='gt'?'lt':'gt',bound]);
 let inner=statics.number.innerType();for(const [name,limit] of steps)inner=inner[name](limit);
 const schema=statics.number.pipe(inner);
 for(const value of values){
  const input=encode(value);
  cases.push({steps:steps.map(([name,limit])=>limit===undefined?[name]:[name,encode(limit)]),...(input===undefined?{}:{value:input})});
  const result=validator.parse(schema,input);
  if(result.ok){bytes.writeDoubleBE(result.value);expected.push({ok:true,bits:bytes.toString('hex')});}
  else expected.push(JSON.parse(JSON.stringify(result)));
 }
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod v3 f64 bound observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_float_bound_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod v3/native f64 bound comparisons: ordered diagnostics and IEEE output bits including NaN bounds and infinities`);
}
