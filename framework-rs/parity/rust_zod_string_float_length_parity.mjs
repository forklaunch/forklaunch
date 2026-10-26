import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const bounds=[-Infinity,-Number.MAX_VALUE,-1e21,-2,-1,-0.5,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,0.5,1,1+Number.EPSILON,1.5,2,2.5,3,4294967296,Number.MAX_SAFE_INTEGER,Number.MAX_SAFE_INTEGER+1,1e21,1e300,Number.MAX_VALUE,Infinity,NaN];
const inputs=[undefined,null,false,0,[],{},'','a','ab','abc','😀','a😀','İ','ß','ﬃ',' a ',' ß ','ΟΣ','𐐀','a\0b'];
const rows=[],expected=[];
function add(value,checks,borrowed){let schema=statics.string;for(const check of checks)schema=check.argument===undefined?schema[check.name]():schema[check.name](Number(check.argument),check.message);rows.push({float:true,borrowed,value,checks});expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));}
const text=value=>Object.is(value,-0)?'-0':String(value);
for(const name of ['min','max','length'])for(const bound of bounds)for(const value of inputs)for(const message of [undefined,'','quoted "\n\0😀'])for(const borrowed of [false,true])add(value,[{name,argument:text(bound),message}],borrowed);
for(const a of bounds)for(const b of [-1,0,1.5,2,Infinity,NaN])for(const value of ['',' ß ','😀','İ']) {
 const checks=[{name:'min',argument:text(a)},{name:'trim'},{name:'length',argument:text(b)}];
 add(value,checks,true);add(value,[{name:'toUpperCase'},...checks,{name:'toLowerCase'},{name:'max',argument:'1.5'}],false);
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY!=='1'){
 const result=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_case_parity'],{input:rows.map(row=>JSON.stringify(row)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});assert.equal(result.status,0,result.stderr);
 const actual=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(actual.length,rows.length);for(let i=0;i<rows.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(rows[i]));
}
console.log(`PASS ${rows.length} actual-source${process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1'?'':'/native'} general UTF16 string-length comparisons (borrowed + allocating, ordered IEEE bounds/messages)`);
