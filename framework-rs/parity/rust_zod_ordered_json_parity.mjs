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
 case 'strict':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)]))).strict();
 case 'object':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)])));
 case 'string':return statics.string;
 case 'number':return statics.number;
 case 'boolean':return statics.boolean;
 case 'unknown':return statics.unknown;
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
const strict={kind:'strict',fields:[['known',{kind:'string'}],['n',{kind:'optional',inner:{kind:'number'}}]]};
const strictEmpty={kind:'strict',fields:[]};
const nested={kind:'object',fields:[['outer',strict]]};
const schemas=[strict,strictEmpty,nested,{kind:'strict',fields:[['10',{kind:'number'}],['2',{kind:'string'}],['a',{kind:'boolean'}],['01',{kind:'string'}]]},{kind:'strict',fields:[['outer',strict]]},{kind:'array',inner:strict},
 {kind:'optional',inner:strict},{kind:'nullable',inner:strict},
 {kind:'union',branches:[strict,{kind:'boolean'}]}, {kind:'union',branches:[strict,{kind:'unknown'}]},
 {kind:'union',branches:[{kind:'string'},strict]}, {kind:'array',inner:{kind:'union',branches:[strict,strictEmpty]}}];
const raws=['-1e-400','1e-400','[-0,-0.0,-0e100,1e-400,-1e-400,9007199254740993,18446744073709551615]','{"3":"-0","7":-0,"8":-1e-400}','null','true','false','0','-0','9007199254740993','1e300','"x"','[]','{}','{"known":"yes","n":"2"}',
 '{"known":2,"z":1,"a":2}', '{"z":0,"a":0,"z":1,"known":"yes"}',
 '{"$serde_json::private::Number":"1e309","$serde_json::private::RawValue":"0"}',
 '{"__proto__":1,"constructor":2,"toString":3}', '{"known":"yes","n":"bad"}'];
for(const keys of [['z','a'],['a','z'],['10','2','01','0','-0','4294967294','4294967295'],["a'b",'a"b','a\nb','😀','\u2028','']]){
 for(const known of ['"yes"','2','null']){const raw='{'+[...keys.map((key,index)=>JSON.stringify(key)+':'+index),'"known":'+known].join(',')+'}';
 raws.push(raw,'{"outer":'+raw+'}', '['+raw+','+raw+']');}
}
const cases=[],expected=[];
for(const schema of schemas)for(const raw of raws)for(const diagnosticPath of [[],['body'],['request','body']]){
 let actual=build(schema),input=JSON.parse(raw);
 for(const key of [...diagnosticPath].reverse()){actual=z.object({[key]:actual});input={[key]:input};}
 const parsed=validator.parse(actual,input);
 cases.push({ordered:true,schema,raw,path:diagnosticPath});
 if(parsed.ok){let value=parsed.value;for(const key of diagnosticPath)value=value[key];expected.push({ok:true,absent:false,value:tag(value)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
// Missing ordering is irrelevant when a later valid branch wins. Native plain
// Value callers must retain that actual success instead of eagerly Unsupported.
for(const raw of ['{"known":"yes","z":1,"a":2}','{"known":false,"z":1,"a":2}']){
 const schema={kind:'union',branches:[strict,{kind:'unknown'}]},parsed=validator.parse(build(schema),JSON.parse(raw));
 assert.equal(parsed.ok,true);cases.push({ordered:false,schema,raw});expected.push({ok:true,absent:false,value:tag(parsed.value)});
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual-source ordered strict observations`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 // Upstream serde_json (fb1e03d21) reserves literal `$serde_json::private::*` object
 // keys under arbitrary_precision; those raw inputs are covered by the default pass.
 const selected=cases.map((_,index)=>index).filter(index=>!features.length||!String(cases[index].raw??'').includes('$serde_json::private::'));
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:selected.map(index=>JSON.stringify(cases[index])).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,selected.length);
 for(let position=0;position<selected.length;position++){const index=selected[position];assert.deepEqual(output[position],expected[index],JSON.stringify(cases[index]));}
 console.log(`PASS ${cases.length} actual-source/native ordered strict comparisons (${features.length?'arbitrary_precision':'default'}; own keys, duplicates, nested unions, raw paths, IEEE output)`);
}
