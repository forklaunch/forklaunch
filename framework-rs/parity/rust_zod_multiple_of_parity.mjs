import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const divisors=[0,-0,-1,-.1,NaN,Infinity,-Infinity,Number.MIN_VALUE,1e-308,1e-100,1e-8,1e-7,1.2e-7,1.234567890123456e-7,1e-6,.00001,.001,.01,.1,.2,.25,.3,.5,1,2,3,7,10,99.9,1e15,9007199254740991,1e20,1e21,1.2e21,1e100,Number.MAX_VALUE];
const values=[undefined,null,false,true,'',[],[null],['0.3'],{},0,'-0',Number.MIN_VALUE,1e-308,1e-7,1.2e-7,1e-6,.1,.2,.3,.1+.2,1.005,2.55,12.99,100.01,9007199254740991,9007199254740992,1000000000000000128,1e20,1e21,1.2e21,1e100,Number.MAX_VALUE,'Infinity','-Infinity','NaN'];
const bytes=Buffer.alloc(8);
function adjacent(value,delta){bytes.writeDoubleBE(value);bytes.writeBigUInt64BE(bytes.readBigUInt64BE()+BigInt(delta));return bytes.readDoubleBE();}
for(const divisor of divisors)for(const factor of [.5,1,2,3,10]){
 const value=divisor*factor;
 if(!Number.isFinite(value)||value===0)continue;
 for(const candidate of [adjacent(value,-1),value,adjacent(value,1)])if(Number.isFinite(candidate))values.push(candidate,-candidate);
}
// Deterministic finite IEEE samples span tiny subnormals, fixed and exponent forms.
let seed=42n;const mask=(1n<<64n)-1n;
for(let i=0;i<400;i++){seed=(seed*6364136223846793005n+1442695040888963407n)&mask;bytes.writeBigUInt64BE(seed);const value=bytes.readDoubleBE();if(Number.isFinite(value))values.push(value);}
const cases=[],expected=[];
for(const divisor of divisors)for(const sequence of ['multiple','intFirst','intLast','finiteFirst']){
 let schema=statics.number.innerType();
 if(sequence==='intFirst')schema=schema.int();
 if(sequence==='finiteFirst')schema=schema.finite();
 schema=schema.multipleOf(divisor);
 if(sequence==='intLast')schema=schema.int();
 schema=statics.number.pipe(schema);
 for(const value of values){
  const input=Object.is(value,-0)?'-0':value;
  const encodedDivisor=Object.is(divisor,-0)?'-0':Number.isFinite(divisor)?divisor:String(divisor);
  cases.push({divisor:encodedDivisor,sequence,...(input===undefined?{}:{value:input})});
  const result=validator.parse(schema,input);
  if(result.ok){bytes.writeDoubleBE(result.value);expected.push({ok:true,bits:bytes.toString('hex')});}
  else expected.push(JSON.parse(JSON.stringify(result)));
 }
}
// These are valid upstream registrations, despite JSON Schema positivity rules.
for(const divisor of [0,-0,-1,-.1,NaN,Infinity,-Infinity]){
 const schema=statics.number.pipe(statics.number.innerType().multipleOf(divisor));
 assert.ok(schema);
 const result=validator.parse(schema,1);
 assert.equal(result.ok,divisor===-1||divisor===-.1);
}
console.log('PASS7 actual Zod zero/negative/nonfinite divisor registrations included in the native differential matrix');
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod v3 multipleOf observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_multiple_of_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod v3/native multipleOf comparisons: complete ordered diagnostics and IEEE output bits; scientific-notation behavior retained`);
}
