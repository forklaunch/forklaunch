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
 let result;
 switch(spec.kind){
 case 'string':result=statics.string;break;
 case 'number':result=statics.number.innerType();break;
 case 'boolean':return statics.boolean;
 case 'unknown':return statics.unknown;
 case 'union':return validator.union(spec.branches.map(build));
 case 'literal':return validator.literal(spec.literal.kind==='number'?(typeof spec.literal.value==='string'?Number(spec.literal.value):spec.literal.value):spec.literal.value);
 case 'optional':return build(spec.inner).optional();
 case 'nullable':return build(spec.inner).nullable();
 case 'default':return build(spec.inner).default(spec.value);
 case 'array':result=build(spec.inner).array();break;
 case 'object':return z.object(Object.fromEntries(spec.fields.map(([key,value])=>[key,build(value)])));
 default:throw Error('unknown schema');
 }
 for(const check of spec.checks??[]){const message=check.message===undefined?undefined:{message:check.message};result=check.argument===undefined?result[check.name](message):result[check.name](check.argument,message);}
 return spec.kind==='number'?statics.number.pipe(result):result;
}
const string={kind:'string',checks:[{name:'trim'},{name:'min',argument:2}]};
const number={kind:'number',checks:[{name:'int'},{name:'finite'}]};
const bool={kind:'boolean'},unknown={kind:'unknown'};
const wrap=(kind,inner,extra={})=>({kind,inner,...extra});
const object={kind:'object',fields:[['name',string],['age',number],['flag',wrap('optional',bool)],['label',wrap('default',{kind:'string'},{value:'default'})]]};
const schemas=[string,number,bool,unknown,object,{kind:'object',fields:[]},wrap('array',object,{checks:[{name:'min',argument:2},{name:'max',argument:1}]}),wrap('array',string,{checks:[{name:'min',argument:3},{name:'length',argument:2},{name:'min',argument:1}]}),{kind:'object',fields:[['10',string],['2',number],['a',bool],['01',string]]},{kind:'object',fields:[['items',wrap('array',object)],['other',wrap('optional',unknown)]]}];
for(const base of [string,number,bool,unknown,object])schemas.push(wrap('optional',base),wrap('nullable',base),wrap('default',base,{value:base===object?{name:' ab ',age:'3'}:base===number?'2':base===bool?'true':' xx '}),wrap('optional',wrap('default',base,{value:'d'})),wrap('default',wrap('optional',base),{value:'d'}),wrap('nullable',wrap('optional',base)));
// Ordered unions retain source dirty/aborted status and one-level raw issue formatting.
const union=(...branches)=>({kind:'union',branches});
const strictString={kind:'string'};
const unions=[union(),union(strictString),union(string,number),
 union({kind:'string',checks:[{name:'min',argument:10}]},{kind:'string',checks:[{name:'trim'}]}),
 union({kind:'string',checks:[{name:'min',argument:5}]},{kind:'string',checks:[{name:'min',argument:8}]}),
 union(wrap('array',string),object),union(wrap('optional',string),unknown),
 union(wrap('default',strictString,{value:'fallback'}),bool),
 union(union(string,wrap('array',number)),object),
 union({kind:'object',fields:[['name',string]]},{kind:'object',fields:[['age',number]]}),
 union(wrap('nullable',bool),string),union(number,unknown),union(unknown,number)];
for(const schema of unions)schemas.push(schema,wrap('optional',schema),wrap('nullable',schema),wrap('array',schema),
 {kind:'object',fields:[['outer',schema]]},
 {kind:'object',fields:[['outer',wrap('array',{kind:'object',fields:[['inner',schema]]})]]});
const values=[undefined,null,false,true,0,.5,'','x',' aa ','NaN','-0',[],['x'],[' aa ','b'],[null,1],{}, {name:' aa ',age:'2',extra:true},{name:' a ',age:'bad',flag:null},{name:'long',age:2.5,label:null},{'10':null,'2':'bad',a:1,'01':false},{items:[{name:' aa ',age:'2',extra:1}],extra:{nested:true}},JSON.parse('{"__proto__":{"x":1},"name":" aa ","age":"2"}')];
values.push([{name:' a ',age:'bad'}, {name:' aa ',age:3}], {outer:false},{outer:[{inner:false},{inner:[]}]},{outer:[{inner:{name:null,age:'bad'}}]},[false,null],{outer:{name:null,age:'bad'}}, {outer:' x '});
for(let i=0;i<64;i++)values.push({name:i%2?' aa ':'x',age:i%3?String(i):'invalid',flag:i%4===0?null:i%2?'true':false,extra:i});
const bytes=Buffer.alloc(8);
function tag(value){if(typeof value==='number'){bytes.writeDoubleBE(value);return {$number:bytes.toString('hex')};}if(Array.isArray(value))return value.map(tag);if(value!==null&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,tag(item)]));return value;}
const cases=[],expected=[];
for(const schema of schemas){const actual=build(schema);for(const value of values){
 const input={schema,...(value===undefined?{}:{value})},before=JSON.stringify(input),parsed=validator.parse(actual,value);
 cases.push(input);expected.push(parsed.ok?(parsed.value===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(parsed.value)}):JSON.parse(JSON.stringify(parsed)));
 assert.equal(JSON.stringify(input),before);
}}
// Evaluate diagnostics in a trusted request path without cloning native input.
for(const schema of unions)for(const path of [['body'],['request','body']])for(const value of values){
 let wrapped=build(schema),input=value;
 for(const key of [...path].reverse()){wrapped=z.object({[key]:wrapped});input={[key]:input};}
 const parsed=validator.parse(wrapped,input);
 cases.push({schema,path,...(value===undefined?{}:Object.is(value,-0)?{raw:'-0'}:{value})});
 if(parsed.ok){let output=parsed.value;for(const key of path)output=output[key];expected.push(output===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(output)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
// Typed literal factory and explicit ordered enum lowering (no object reflection
// is inferred by the native evaluator). Compare numeric output through IEEE tags.
const literal=value=>({kind:'literal',literal:{kind:typeof value,value:typeof value==='number'&&!Number.isFinite(value)?String(value):Object.is(value,-0)?'-0':value}});
const literalValues=[true,false,0,-0,1,-1,.5,1e-7,1e-6,1e20,1e21,Number.MIN_VALUE,Number.MAX_VALUE,9007199254740992,Number.MAX_SAFE_INTEGER,NaN,Infinity,-Infinity,
 ...Array.from({length:256},(_,index)=>String.fromCharCode(index)),'é','😀','\u2028','\u2029','a"\\\n\u0000'];
function addLiteralCase(schema,value,path=[]){
 let actual=build(schema),wrapped=value;
 for(const key of [...path].reverse()){actual=z.object({[key]:actual});wrapped={[key]:wrapped};}
 const parsed=validator.parse(actual,wrapped);cases.push({schema,path,...(value===undefined?{}:Object.is(value,-0)?{raw:'-0'}:{value})});
 if(parsed.ok){let output=parsed.value;for(const key of path)output=output[key];expected.push(output===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(output)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
for(const value of literalValues){const spec=literal(value);for(const input of [...values,...literalValues.filter(value=>typeof value!=='number'||Number.isFinite(value))])addLiteralCase(spec,input);
 for(const input of [undefined,false,'other',{outer:false}])addLiteralCase(union(spec,{kind:'string',checks:[{name:'min',argument:10}]}),input,['body']);
}
for(const members of [[],[['a','x']],[['a','x'],['b','y']],[['a',1],['b','1'],['c',false]],[['a','same'],['b','same']],[['2','two'],['1','one'],['z','zee']],[['a',NaN],['b',Infinity],['c',-0]]]){
 const record=Object.fromEntries(members),actual=validator.enum_(record);
 const lowered=union(...Object.values(record).map(literal));
 for(const value of values){const parsed=validator.parse(actual,value);cases.push({schema:lowered,...(value===undefined?{}:{value})});expected.push(parsed.ok?(parsed.value===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(parsed.value)}):JSON.parse(JSON.stringify(parsed)));}
}
for(const expectedValue of [-0,0,9007199254740992,18446744073709551616])for(const raw of ['-0','0','9007199254740993','18446744073709551615']){
 const schema=literal(expectedValue),parsed=validator.parse(build(schema),JSON.parse(raw));cases.push({schema,raw});expected.push(parsed.ok?{ok:true,absent:false,value:tag(parsed.value)}:JSON.parse(JSON.stringify(parsed)));
}
// Raw numeric JSON preserves -0 and unsafe integer spelling until the native
// evaluator normalizes it. Compare IEEE tags, never JSON.stringify(number).
for(const raw of ['9007199254740993','18446744073709551615','-0','[-0,9007199254740993]','{"n":18446744073709551615}', '"-0"']){
 const parsed=validator.parse(statics.unknown,JSON.parse(raw));assert.equal(parsed.ok,true);
 cases.push({schema:unknown,raw});expected.push({ok:true,absent:false,value:tag(parsed.value)});
}
// Unsupported is distinct from validation rejection; do not compare Infinity
// with null or pretend that a successful non-JSON source value failed validation.
for(const value of ['Infinity','-Infinity']){
 assert.equal(validator.parse(statics.number,value).ok,true);
 cases.push({schema:{kind:'number'},value});expected.push({unsupported:'NonFiniteNumber'});
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length-2} actual Zod JSON composition observations plus two explicit nonfinite boundaries`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length-2} actual Zod/native JSON composition comparisons plus two explicit nonfinite boundaries: ordered errors, defaults, strip, atomic transforms and IEEE numeric output`);
}
