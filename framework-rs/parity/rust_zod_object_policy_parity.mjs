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
 case 'policy':{let result=validator.schemify(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)])));result=result[spec.unknown]();return spec.catchall?result.catchall(build(spec.catchall)):result;}
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
function wire(value){
 if(Object.is(value,-0))return '-0';
 if(Array.isArray(value))return '['+value.map(wire).join(',')+']';
 if(value&&typeof value==='object')return '{'+Object.entries(value).map(([key,value])=>JSON.stringify(key)+':'+wire(value)).join(',')+'}';
 return JSON.stringify(value);
}
const wrap=(kind,inner,extra={})=>({kind,inner,...extra});
const values=[undefined,null,false,0,'x',[],{}, {field:'ok'}, {field:0}, {field:'ok',z:'2',a:'3'}, {field:'ok',z:'bad',a:'bad'}, {field:0,z:'bad',a:'bad'}, {field:'ok',z:null,a:true}, {field:'ok',z:{nested:'2'},a:['3']}, JSON.parse('{"field":"ok","10":"1","2":"2","z":"bad","a":"bad"}'),JSON.parse('{"field":"ok","__proto__":"bad","constructor":"2","$serde_json::private::Number":"12"}'),{field:'ok',z:-0,a:9007199254740992}];
const schemas=[];
const catchalls=[undefined,{kind:'never'},wrap('optional',{kind:'never'}),{kind:'unknown'},{kind:'number'},{kind:'nullish'}, {kind:'string',checks:[{name:'min',argument:3}]},wrap('array',{kind:'number'}),{kind:'union',branches:[{kind:'string',checks:[{name:'min',argument:3}]},{kind:'number'}]},{kind:'record',key:{kind:'string'},valueSchema:{kind:'number'}},wrap('default',{kind:'number'},{value:7})];
for(const unknown of ['strip','strict','passthrough'])for(const catchall of catchalls){
 const base={kind:'policy',unknown,fields:[['field',{kind:'string'}]],...(catchall?{catchall}:{})};
 schemas.push(base,wrap('optional',base),wrap('array',base),{kind:'union',branches:[base,{kind:'unknown'}]},{kind:'object',fields:[['field',base]]});
}
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
for(const schema of schemas.filter(schema=>schema.kind==='policy'))for(const raw of [
 '{"field":"ok","z":"2","a":"3","z":"4"}',
 '{"field":0,"z":"bad","a":"bad","z":null}',
 '{"field":"ok","__proto__":"2","__proto__":"bad","constructor":"3"}',
 '{"field":"ok","z":-0,"a":-1e-999}',
 '{"field":"ok","z":{"$serde_json::private::Number":"12"},"a":{"b":2}}',
])for(const diagnosticPath of [[],['body'],['request','body']]){
 let actual=build(schema),input=JSON.parse(raw);
 for(const key of [...diagnosticPath].reverse()){actual=z.object({[key]:actual});input={[key]:input};}
 const parsed=validator.parse(actual,input);cases.push({schema,path:diagnosticPath,ordered:true,raw});
 if(parsed.ok){let output=parsed.value;for(const key of diagnosticPath)output=output[key];expected.push({ok:true,absent:false,value:tag(output)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
const exactCount=cases.length;
for(const unknown of ['strip','strict','passthrough']) {
 const schema={kind:'policy',unknown,fields:[],catchall:{kind:'number'}};
 const input={overflow:'1e309'};const parsed=validator.parse(build(schema),input);
 assert.equal(parsed.ok,true);assert.equal(parsed.value.overflow,Infinity);
 cases.push({schema,ordered:true,raw:JSON.stringify(input)});expected.push({unsupported:'NonFiniteNumber'});
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${exactCount} actual-source object-policy observations`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(output[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${exactCount} actual-source/native object-policy comparisons +3 explicit nonfinite boundaries (${features.length?'arbitrary_precision':'default'})`);
}
