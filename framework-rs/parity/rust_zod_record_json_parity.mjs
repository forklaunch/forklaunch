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
const string={kind:'string'},number={kind:'number'},boolean={kind:'boolean'},unknown={kind:'unknown'};
const trim={kind:'string',checks:[{name:'trim'}]},minimum={kind:'string',checks:[{name:'min',argument:2}]};
const wrap=(kind,inner,extra={})=>({kind,inner,...extra});
const literal=value=>({kind:'literal',literal:{kind:typeof value,value}});
const union=(...branches)=>({kind:'union',branches});
const record=(key,valueSchema)=>({kind:'record',key,valueSchema});
const keys=[string,trim,minimum,{kind:'string',checks:[{name:'trim'},{name:'min',argument:2}]},number,boolean,unknown,literal('a'),literal(1),literal(false),union(literal('a'),literal('b')),union(number,trim),union(minimum,number),union(),wrap('optional',string),wrap('nullable',trim),wrap('default',unknown,{value:'unused'}),wrap('array',unknown),{kind:'object',fields:[]}];
const values=[number,string,boolean,unknown,wrap('optional',string),wrap('nullable',number),wrap('default',string,{value:'fallback'}),{kind:'object',fields:[['n',number],['label',wrap('default',string,{value:'default'})]]},wrap('array',number),union(number,string)];
const raws=['{}','null','[]','"x"','false','0',
 '{"z":"bad","a":"bad","10":"bad","2":"bad"}',
 '{" a ":"1","a":"2","__proto__":"3","constructor":"4"}',
 '{"a":"1"," a ":"2"}','{"z":"1","a":"2","z":"3"}',
 '{"a":null,"b":false}','{"true":"1","false":"2","TRUE":"3"}',
 '{"-0":"1","0":"2","01":"3","1e21":"4","1e-7":"5"}',
 '{"a":{"n":"2"},"b":{"n":"bad"}}','{"a":["2","bad"],"b":[]}',
 '{"__proto__":"bad"," __proto__ ":"bad"}',
 '{"$serde_json::private::Number":"12","$serde_json::private::RawValue":"0"}',
 '{"😀":"2","a\\n":"3","\\u0000":"4","a\\\"b":"5"}',
 '{"4294967294":1,"4294967295":2,"2":3,"01":4}',
 '{"a":-0,"b":9007199254740993}'];
const cases=[],expected=[];
function add(schema,raw,path=[],ordered=true){
 let actual=build(schema),input=JSON.parse(raw);
 for(const key of [...path].reverse()){actual=z.object({[key]:actual});input={[key]:input};}
 const parsed=validator.parse(actual,input);cases.push({ordered,schema,raw,path});
 if(parsed.ok){let value=parsed.value;for(const key of path)value=value[key];expected.push({ok:true,absent:false,value:tag(value)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
for(const key of keys)for(const value of values)for(const raw of raws)for(const path of [[],['body']])add(record(key,value),raw,path);
for(const schema of [record(trim,number),record(string,minimum),record(unknown,{kind:'strict',fields:[['n',number]]})])for(const raw of raws){
 add(union(schema,boolean),raw,['body']);add(union(schema,unknown),raw);
 add({kind:'object',fields:[['outer',schema]]},'{"outer":'+raw+'}',['body']);
 add(wrap('array',schema),'['+raw+','+raw+']');
}
for(const raw of ['{"z":"1","a":"2"}','{"z":"bad","a":"bad"}'])add(union(record(trim,number),unknown),raw,[],false);
for(const raw of ['{"1":"1","01":"2","x":"3"}','{"a":"1","z":"2"}'])add(record(union(number,unknown),number),raw,[],false);
// Existing finite-value boundary stays explicit for native key projection.
for(const value of ['Infinity','-Infinity']){
 const schema=record(number,unknown),raw=JSON.stringify({[value]:1});assert.equal(validator.parse(build(schema),JSON.parse(raw)).ok,true);
 cases.push({ordered:true,schema,raw});expected.push({unsupported:'NonFiniteNumber'});
}
for(const key of [string,literal('allowed')]){
 const schema=record(key,number),raw='{"a":"Infinity"}';
 assert.equal(validator.parse(build(schema),JSON.parse(raw)).ok,key===string);
 cases.push({ordered:true,schema,raw});expected.push({unsupported:'NonFiniteNumber'});
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length-4} actual-source record observations +4 explicit nonfinite key boundaries`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 // Upstream serde_json (fb1e03d21) reserves literal `$serde_json::private::*` object
 // keys under arbitrary_precision; those raw inputs are covered by the default pass.
 const selected=cases.map((_,index)=>index).filter(index=>!features.length||!String(cases[index].raw??'').includes('$serde_json::private::'));
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:selected.map(index=>JSON.stringify(cases[index])).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,selected.length);
 for(let position=0;position<selected.length;position++){const index=selected[position];assert.deepEqual(output[position],expected[index],JSON.stringify(cases[index]));}
 console.log(`PASS ${cases.length-4} actual-source/native record comparisons +4 explicit nonfinite boundaries (${features.length?'arbitrary_precision':'default'})`);
}
