import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const messages=['','custom','"quote"','line\nbreak','💡\u0000'];
const inputs=[undefined,null,{},[],0,.5,-1,1e20,'NaN','Infinity','abc','',' a ','💡'];
const checks=[['number','min',1],['number','max',-1],['number','gt',1],['number','lt',-1],['number','int'],['number','finite'],['number','multipleOf',.3],['number','safe'],['string','min',4],['string','max',0],['string','length',3],['string','startsWith','x'],['string','endsWith','x'],['string','includes','x']];
const cases=[],expected=[],bytes=Buffer.alloc(8);
for(const [type,name,argument] of checks)for(const message of [undefined,...messages])for(const input of inputs)for(const order of ['single','mixed']){
 const entries=[{name,argument,message}];
 if(order==='mixed'){
  if(type==='number')entries.push({name:'safe',message:'safe second'}, {name:'min',argument:3}, {name:'int',message:''});
  else entries.push({name:'trim'}, {name:'min',argument:3,message:'length second'}, {name:'endsWith',argument:'z'});
 }
 let schema=type==='number'?statics.number.innerType():statics.string;
 for(const entry of entries){const options=entry.message===undefined?undefined:{message:entry.message};schema=entry.argument===undefined?schema[entry.name](options):schema[entry.name](entry.argument,options);}
 if(type==='number')schema=statics.number.pipe(schema);
 const result=validator.parse(schema,input);
 cases.push({type,checks:entries,...(input===undefined?{}:{value:input})});
 if(result.ok&&type==='number'){bytes.writeDoubleBE(result.value);expected.push({ok:true,bits:bytes.toString('hex')});}
 else expected.push(JSON.parse(JSON.stringify(result)));
}
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${cases.length} actual ForkLaunch custom diagnostic observations`);
else {
 const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_check_messages_parity'],{input:cases.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(result.status,0,result.stderr);
 const results=result.stdout.trim().split('\n').map(value=>JSON.parse(value));assert.equal(results.length,cases.length);
 for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],expected[index],JSON.stringify(cases[index]));
 console.log(`PASS ${cases.length} actual ForkLaunch Zod/native custom diagnostic comparisons: byte-preserved literal messages, unchanged type errors and ordered atomic output`);
}
