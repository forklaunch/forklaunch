import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const values=[undefined,null,false,true,0,1,{},[],['YQ=='],'','YQ==','YQ','Y','Y=Q','YQ==Yg==',' Y!Q\n','YQ-_','____','====','AA=A','AĀA','AŁA','A💡A','ŁŁ','ĀAAA','ＡＡ'];
for(let cp=0;cp<=0x10ffff;cp+=cp<65536?1:1021){
 if(cp>=0xd800&&cp<=0xdfff)continue;
 const ch=String.fromCodePoint(cp);values.push(ch+'YQ==','A'+ch+'A','YQ'+ch+'AA',ch+ch);
}
for(let length=0;length<=256;length++){
 const bytes=Buffer.from(Array.from({length},(_,index)=>(index*73+length)&255));
 const base=bytes.toString('base64');
 values.push(base,base.replace(/=/g,''),base.replace(/\+/g,'-').replace(/\//g,'_'),base.split('').join('! \n'));
}
const cases=[],expected=[];
for(const value of values){
 const input=value===undefined?{}:{value};const original=JSON.stringify(input);
 const parsed=validator.parse(statics.binary,value);
 expected.push(parsed.ok?{ok:true,bytes:Array.from(parsed.value)}:JSON.parse(JSON.stringify(parsed)));
 assert.equal(JSON.stringify(input),original);cases.push(input);
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch binary factory observations: exhaustive BMP scalar and sampled astral/byte/padding cases`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_binary_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod/native binary comparisons: exact bytes, strict type diagnostics, UTF16 low-byte semantics and unchanged raw input`);
}
