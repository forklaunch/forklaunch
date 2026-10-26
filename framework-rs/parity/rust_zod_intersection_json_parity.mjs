import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const {z}=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'))('zod/v3');
function build(spec){switch(spec.kind){
 case 'intersection':return build(spec.left).and(build(spec.right));
 case 'record':return validator.record(build(spec.key),build(spec.valueSchema));
 case 'strict':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)]))).strict();
 case 'object':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)])));
 case 'string':{let result=statics.string;for(const c of spec.checks??[])result=c.argument===undefined?result[c.name]():result[c.name](c.argument);return result;}
 case 'number':{let result=statics.number;for(const c of spec.checks??[])result=result.pipe(c.argument===undefined?z.number()[c.name]():z.number()[c.name](c.argument));return result;}
 case 'tuple':{let result=z.tuple(spec.items.map(build));return spec.rest?result.rest(build(spec.rest)):result;}
 case 'boolean':return statics.boolean;
 case 'unknown':return statics.unknown;
 case 'literal':return validator.literal(spec.literal.value);
 case 'default':return build(spec.inner).default(spec.value);
 case 'optional':return build(spec.inner).optional();
 case 'nullable':return build(spec.inner).nullable();
 case 'array':return z.array(build(spec.inner));
 case 'union':return validator.union(spec.branches.map(build));
 default:throw Error(spec.kind);
}}
function tag(value){if(typeof value==='number'){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {$number:bytes.toString('hex')};}
 if(Array.isArray(value))return value.map(tag);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,tag(value)]));
 return value;
}
const string={kind:'string'},unknown={kind:'unknown'},number={kind:'number'};
const trim={kind:'string',checks:[{name:'trim'}]};
const dirtyTrim={kind:'string',checks:[{name:'min',argument:4},{name:'trim'}]};
const object=(inner)=>({kind:'object',fields:[['nested',inner]]});
const wrap=(kind,inner)=>({kind,inner});
const record=(valueSchema)=>({kind:'record',key:string,valueSchema});
const branches=[string,trim,dirtyTrim,{kind:'string',checks:[{name:'max',argument:1}]},unknown,number,{kind:'boolean'},wrap('optional',string),wrap('nullable',string),wrap('array',trim),wrap('array',dirtyTrim),wrap('array',unknown),object(unknown),object(trim),object(dirtyTrim),record(unknown),record(trim),record(dirtyTrim),{kind:'default',inner:unknown,value:{nested:1}},{kind:'default',inner:string,value:'default'}];
branches.push({kind:'number',checks:[{name:'int'}]},{kind:'number',checks:[{name:'min',argument:3}]},{kind:'tuple',items:[trim]},{kind:'tuple',items:[dirtyTrim],rest:unknown});
const raws=['null','false','0','-0','"2"','" x "','"xxx"','[]','[" x "]','[" x ",false]','{}','{"nested":" x ","extra":1}','{"nested":{"__proto__":{"x":1},"z":2}}','{"__proto__":{"x":1},"constructor":2}','{"z":" x ","a":"y"}','{"a":"y","z":" x "}','{"2":" x ","1":"y"}','{"$serde_json::private::Number":"12"}'];
const cases=[],expected=[];
function add(schema,raw,path=[]){
 let actual=build(schema),input=raw===undefined?undefined:JSON.parse(raw);
 for(const key of [...path].reverse()){actual=z.object({[key]:actual});input={[key]:input};}
 const parsed=validator.parse(actual,input);cases.push({ordered:raw!==undefined,schema,raw,path});
 if(parsed.ok){let value=parsed.value;for(const key of path)value=value[key];expected.push(value===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(value)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
for(const left of branches)for(const right of branches)for(const raw of raws){
 const schema={kind:'intersection',left,right};add(schema,raw);
}
for(const left of branches)for(const right of branches){
 const schema={kind:'intersection',left,right};
 add(schema,undefined);
 add(schema,'" x "',['body']);
 add({kind:'union',branches:[schema,unknown]},'{"nested":" x "}',['body']);
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual-source intersection observations`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(output[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual-source/native intersection comparisons (${features.length?'arbitrary_precision':'default'})`);
}
