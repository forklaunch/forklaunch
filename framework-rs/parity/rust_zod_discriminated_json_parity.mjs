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
 case 'enum':return z.enum(spec.values);
 case 'nativeEnum':return z.nativeEnum(Object.fromEntries(spec.entries.map(([key,value])=>[key,value.kind==='number'?Number(value.value):value.value])));
 case 'discriminated':return z.discriminatedUnion(spec.discriminator,spec.options.map(build));
 case 'null':return z.null();
 case 'undefined':return z.undefined();
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
const wrap=(kind,inner,extra={})=>({kind,inner,...extra});
const literal=value=>({kind:'literal',literal:{kind:typeof value,value}});
const native=entries=>({kind:'nativeEnum',entries:entries.map(([key,value])=>[key,{kind:typeof value,value:Number.isNaN(value)?'NaN':value===Infinity?'Infinity':value===-Infinity?'-Infinity':value}])});
const leaves=[{kind:'enum',values:[]},{kind:'enum',values:['a']},{kind:'enum',values:['a','b']},{kind:'enum',values:['a','a']},{kind:'enum',values:['__proto__','constructor','prototype']},{kind:'enum',values:["'\n😀",'']},native([]),native([['A','a'],['B','b']]),native([['0','A'],['1','B'],['A',0],['B',1]]),native([['10','ten'],['2','two'],['z','z']]),native([['__proto__','proto'],['constructor','ctor'],['prototype','p']]),native([['A',1],['B','b']]),native([['NaN',NaN],['Infinity',Infinity],['N',-Infinity]]),native([['A','A']]),native([['1',2],['A',1]]),native([['A',0],['B',-0]]),native([['A',NaN],['B',NaN]])];
const tagSchemas=[...leaves,literal('a'),literal('b'),literal(0),literal(true),{kind:'null'},{kind:'undefined'},wrap('optional',literal('a')),wrap('nullable',literal('a')),wrap('default',literal('a'),{value:'a'}),wrap('optional',string),wrap('nullable',unknown),wrap('default',wrap('optional',literal('a')),{value:'a'}),{kind:'union',branches:[literal('a'),literal('b')]}];
const inputs=[undefined,null,false,true,0,-0,1,2,'0','a','b','A','B','c','proto','ctor','p','two','ten','z','',"'\n😀",[],{}];
// Preserve numeric input identity; JSON.stringify would erase nested negative zero.
function wire(value){
 if(Object.is(value,-0))return '-0';
 if(Array.isArray(value))return '['+value.map(item=>wire(item)??'null').join(',')+']';
 if(value&&typeof value==='object')return '{'+Object.entries(value).filter(([,item])=>item!==undefined).map(([key,item])=>JSON.stringify(key)+':'+wire(item)).join(',')+'}';
 return JSON.stringify(value);
}
const cases=[],expected=[];let registrationFailures=0;
function add(schema,input,path=[],rawOverride){
 let actual;
 try{actual=build(schema);}catch(error){
  const reason=error.message.includes('duplicate value')?'DuplicateDiscriminator':'DiscriminatorSchema';
  cases.push({schema,value:input,path});expected.push({unsupported:reason});registrationFailures++;return;
 }
 const raw=rawOverride??wire(input);
 // Compare representable JSON input, not an own property containing JS undefined.
 let wrapped=raw===undefined?undefined:JSON.parse(raw);
 for(const key of [...path].reverse()){actual=z.object({[key]:actual});wrapped={[key]:wrapped};}
 const parsed=validator.parse(actual,wrapped);
 cases.push({schema,raw,path,ordered:raw!==undefined});
 if(parsed.ok){let value=parsed.value;for(const key of path)value=value[key];expected.push(value===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(value)});}
 else expected.push(JSON.parse(JSON.stringify(parsed)));
}
for(const leaf of leaves)for(const input of inputs)for(const path of [[],['body']])add(leaf,input,path,Object.is(input,-0)?'-0':undefined);
const object=kind=>({kind:'object',fields:[['kind',kind],['value',wrap('default',number,{value:2})],['label',wrap('optional',{kind:'string',checks:[{name:'trim'}]})]]});
for(const kind of tagSchemas)for(const options of [[object(kind)],[object(kind),object(literal('c'))],[object(kind),object(kind)]]){
 const schema={kind:'discriminated',discriminator:'kind',options};
 for(const input of inputs)for(const path of [[],['body']])add(schema,{kind:input,value:'2',label:' x ',extra:true},path);
 for(const input of [undefined,null,[],0,'a',{}, {kind:'a',value:'bad'}])add(schema,input);
}
for(const schema of [{kind:'discriminated',discriminator:'kind',options:[]},{kind:'discriminated',discriminator:'kind',options:[string]},{kind:'discriminated',discriminator:'kind',options:[{kind:'object',fields:[]}]}])for(const input of inputs)add(schema,input);
let prototypeBoundaries=0;
for(const key of ['__proto__','constructor','toString']) {
 const schema={kind:'discriminated',discriminator:key,options:[{kind:'object',fields:[[key,wrap('optional',literal('a'))]]}]};
 const actual=build(schema);
 assert.equal(validator.parse(actual,{}).ok,false,'inherited property is not undefined');
 const input=Object.fromEntries([[key,'a']]);assert.equal(validator.parse(actual,input).ok,true);
 for(const value of [{},input]){cases.push({schema,raw:JSON.stringify(value),ordered:true});expected.push({unsupported:'PrototypeField'});prototypeBoundaries++;}
}
let selected=0,unselected=0,defaults=0;
const counted=z.discriminatedUnion('kind',[
 z.object({kind:z.literal('a'),value:z.string().transform(value=>{selected++;return value.trim();})}),
 z.object({kind:z.literal('b'),value:z.string().transform(value=>{unselected++;return value;})}),
 z.object({kind:z.literal('c').optional().default(()=>{defaults++;return 'c';})}),
]);
assert.equal(selected+unselected+defaults,0);
assert.equal(validator.parse(counted,{kind:'a',value:' x '}).ok,true);
assert.equal(selected,1);assert.equal(unselected,0);assert.equal(defaults,0);
assert.equal(validator.parse(counted,{}).ok,true);assert.equal(defaults,1);
assert.equal(validator.parse(counted,{kind:'missing'}).ok,false);assert.equal(selected+unselected+defaults,2);
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length-prototypeBoundaries} actual-source enum/discriminator observations (${registrationFailures} constructor rejections) +${prototypeBoundaries} explicit prototype-key boundaries`);
else for(const features of [[],['--features','serde_json/arbitrary_precision']]){
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_json_parity',...features],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);const output=result.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(output.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(output[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length-prototypeBoundaries} actual-source/native enum/discriminator comparisons +${prototypeBoundaries} explicit prototype-key boundaries (${registrationFailures} registration failures, ${features.length?'arbitrary_precision':'default'})`);
}
