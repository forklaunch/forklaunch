import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createRequire} from 'node:module';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator}=actualValidator(root,undefined,{sameRealm:true});
const {z}=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'))('zod/v3');
const keyword='x-forklaunch-zod-diagnostics';
let calls=0;
const defaulted=z.object({flag:z.literal('ok')}).optional().default(()=>{calls++;return {flag:'ok'}});
const fieldDefault=z.string().default(()=>{calls++;return 'fallback'});
const diagnosticDefaults=new Map([[defaulted,{flag:'ok'}],[fieldDefault,'fallback']]);
const schemas=[z.object({flag:z.literal('ok')}),defaulted,z.object({z:z.string(),a:z.literal(1),first:z.boolean()}),z.number().int().min(2).max(1,'custom'),z.string().min(2).max(1),z.array(z.literal('ok')).min(2).max(1),z.union([z.object({a:z.literal('a')}),z.object({a:z.literal('b')})]),z.tuple([z.string(),z.number()]),z.enum(['a','b']),z.nativeEnum({0:'A',A:0,B:'b'}),z.record(z.literal('ok')),z.null(),z.undefined(),z.never(),z.literal(null),z.literal(undefined),z.object({z:z.string(),a:z.literal(1)}).strict(),z.object({label:fieldDefault}),z.record(z.enum(['a','z']),z.literal('ok'))];
const values=[undefined,null,false,true,0,1,2,'','a','ok',[],['ok'],{}, {flag:'ok',extra:1},{flag:'bad'},{z:0,a:2,first:'true'},JSON.parse('{"z":1,"a":2,"10":3,"2":4}'),JSON.parse('{"__proto__":"bad","constructor":"bad"}')];
const rows=[];
for(const source of schemas){
 const before=calls;
 const schema=validator.openapi2020(source,{zodDiagnostics:true,zodChecks:true,diagnosticDefaults});
 assert.equal(calls,before,'export executed default');
 assert.equal(schema[keyword].version,1);
 for(const value of values){const parsed=validator.parse(source,value);rows.push({provenance:schema[keyword],...(value===undefined?{}:{value,raw:JSON.stringify(value)}),expected:parsed.ok?{ok:true,...(parsed.value===undefined?{absent:true}:{absent:false,value:parsed.value})}:JSON.parse(JSON.stringify(parsed))});}
}
assert.throws(()=>validator.openapi2020(defaulted,{zodDiagnostics:true}),/identity-bound/);
const signedDefault=z.number().default(-0);
assert.throws(()=>validator.openapi2020(signedDefault,{zodDiagnostics:true,diagnosticDefaults:new Map([[signedDefault,-0]])}),/negative zero/);
for(const source of [z.string().transform(()=>{throw Error('PRIVATE')}),z.preprocess(()=>{throw Error('PRIVATE')},z.string()),z.lazy(()=>{throw Error('PRIVATE')})])assert.throws(()=>validator.openapi2020(source,{zodDiagnostics:true}),/Unsupported|runtime binding/);
const overridden=z.string().min(3).openapi({minLength:0});
assert.equal(validator.openapi2020(overridden).minLength,0);
assert.throws(()=>validator.openapi2020(overridden,{zodDiagnostics:true}),/validation annotation/);
const source=z.object({z:z.literal('z'),a:z.literal('a')});
assert.deepEqual(validator.openapi2020(source,{zodDiagnostics:true})[keyword].schema.fields.map(field=>field.name),['z','a']);
console.log('PASS '+rows.length+' actual diagnostic source observations; default export calls=0, source order and explicit override/effect guards');

if (process.env.FORKLAUNCH_DIAGNOSTIC_NATIVE === '1') {
 for (const features of [[], ['--features','serde_json/arbitrary_precision']]) {
  const result=spawnSync('cargo',['run','--quiet','--locked','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_provenance_parity',...features],{input:rows.map(({expected,...row})=>JSON.stringify(row)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
  assert.equal(result.status,0,result.stderr);
  const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));
  assert.equal(output.length,rows.length);
  for(let index=0;index<rows.length;index++)assert.deepEqual(output[index],rows[index].expected,'row '+index+': '+JSON.stringify(rows[index]));
  console.log('PASS '+rows.length+' native diagnostic source comparisons ('+(features.length?'arbitrary_precision':'default')+')');
 }
}
