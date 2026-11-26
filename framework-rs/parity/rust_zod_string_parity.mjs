import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
// actualValidator loads this repository's ZodSchemaValidator and static factory,
// whose source imports zod/v3; this intentionally does not assert AJV semantics.
const {validator,statics}=actualValidator(root);
const values=[undefined,null,false,true,0,1,[],{},['a'],{text:'a'},'', 'a','ab','abc','💡','💡a','e\u0301','👩‍💻','\0','\n',' a ','💡💡'];
for(const whitespace of ['\t','\n','\v','\f','\r',' ','\u00a0','\u1680','\u2000','\u2001','\u2002','\u2003','\u2004','\u2005','\u2006','\u2007','\u2008','\u2009','\u200a','\u2028','\u2029','\u202f','\u205f','\u3000','\ufeff','\u0085','\u180e','\u200b'])values.push(whitespace,whitespace+'a'+whitespace,whitespace+'💡'+whitespace);
for(const point of [0x7f,0x80,0x7ff,0x800,0xd7ff,0xe000,0xffff,0x10000,0x10ffff])values.push(String.fromCodePoint(point));
const operations=[['trim'],...['min','max','length'].flatMap(kind=>[0,1,2,3,4294967295].map(limit=>[kind,limit]))];
const chains=[[],...operations.map(step=>[step]),...operations.flatMap(first=>operations.map(second=>[first,second])),[['trim'],['min',2],['trim'],['max',1]]];
const cases=chains.flatMap(steps=>values.map(value=>({steps,...(value===undefined?{}:{value})})));
const expected=chains.flatMap(steps=>{let schema=statics.string;for(const [kind,limit] of steps)schema=schema[kind](...(kind==='trim'?[]:[limit]));return values.map(value=>JSON.parse(JSON.stringify(validator.parse(schema,value))));});
if(process.env.FORKLAUNCH_STRING_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch Zod v3 strict string outcomes`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_string_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod v3/native strict string outcomes: UTF-16 length, ordered checks/trim, default diagnostics and unchanged JSON input domain`);
}
