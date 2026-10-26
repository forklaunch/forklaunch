import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const {z}=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'))('zod/v3');
function build(spec){
 if(['null','undefined','void','never','nullish'].includes(spec.kind))return validator[spec.kind];
 switch(spec.kind){
 case 'tuple': {let schema=z.tuple(spec.items.map(build));return spec.rest?schema.rest(build(spec.rest)):schema;}
 case 'record':return validator.record(build(spec.key),build(spec.valueSchema));
 case 'strict':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)]))).strict();
 case 'object':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)])));
 case 'string':{let result=statics.string;for(const c of spec.checks??[])result=c.argument===undefined?result[c.name]():result[c.name](c.argument);return result;}
 case 'number':return statics.number;
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
const wrap=(kind,inner,extra={})=>({kind,inner,...extra});
const values=[undefined,null,false,true,0,-0,'','x',{},[],['x'],[null],['2'],[false],[' x ', '2'],['x',false],['x',{}],['x',1,2],['x',1,'3'],['x',1,'bad'],[[],{}],[{field:'2'}],[{field:'bad'}],[[1,2]],[['2','3']]];
const string={kind:'string',checks:[{name:'trim'},{name:'min',argument:2}]},number={kind:'number'},unknown={kind:'unknown'};
const slots=[string,number,{kind:'boolean'},wrap('optional',string),wrap('default',number,{value:2}),{kind:'object',fields:[['field',wrap('default',number,{value:2})]]},{kind:'union',branches:[string,number]}];
const schemas=[{kind:'tuple',items:[]},{kind:'tuple',items:[],rest:number}];
for(const item of slots)for(const rest of [undefined,number,unknown,{kind:'never'}]){
 const tuple={kind:'tuple',items:[item],...(rest?{rest}:{})};
 schemas.push(tuple,{kind:'tuple',items:[string,item],...(rest?{rest}:{})},wrap('optional',tuple),wrap('nullable',tuple),wrap('array',tuple),{kind:'object',fields:[['field',tuple]]},{kind:'union',branches:[tuple,unknown]},{kind:'union',branches:[tuple,{kind:'tuple',items:[number]}]},{kind:'union',branches:[{kind:'tuple',items:[number]},tuple]});
}
function wire(value){if(typeof value==='number')return Object.is(value,-0)?'-0':JSON.stringify(value);if(Array.isArray(value))return '['+value.map(wire).join(',')+']';if(value&&typeof value==='object')return '{'+Object.entries(value).map(([k,v])=>JSON.stringify(k)+':'+wire(v)).join(',')+'}';return JSON.stringify(value);}
const cases=[],expected=[];
for(const schema of schemas)for(const value of values)for(const diagnosticPath of [[],['body'],['request','body']]){
 let actual=build(schema),input=value;
 for(const key of [...diagnosticPath].reverse()){actual=z.object({[key]:actual});input={[key]:input};}
 const parsed=validator.parse(actual,input);
 const raw=value===undefined?undefined:wire(value);
 cases.push({schema,path:diagnosticPath,...(raw===undefined?{}:{ordered:true,raw})});
 if(parsed.ok){let output=parsed.value;for(const key of diagnosticPath)output=output[key];expected.push(output===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(output)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual-source tuple observations`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(output[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual-source/native tuple comparisons (${features.length?'arbitrary_precision':'default'})`);
}
