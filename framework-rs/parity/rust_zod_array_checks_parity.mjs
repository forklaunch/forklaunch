import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {validator,statics}=actualValidator(root);
const bounds=[-Infinity,-Number.MAX_VALUE,-2,-1,-0.5,-Number.MIN_VALUE,-0,0,Number.MIN_VALUE,0.5,1,1.5,2,2.5,4294967296,Number.MAX_SAFE_INTEGER+1,Number.MAX_VALUE,Infinity,NaN];
const values=[undefined,null,false,0,'array',{},[],[null],[' a '],[' a ',' b '],[false,' b '],[['nested'],{}]];
const text=value=>Object.is(value,-0)?'-0':String(value);
const operations=[{kind:'nonempty'},...['min','max','length'].flatMap(kind=>bounds.map(bound=>({kind,bound:text(bound)})))];
const chains=[[],...operations.flatMap(check=>[undefined,'','quote"\n\0😀'].map(message=>[{...check,message}])),...operations.flatMap(first=>operations.map(second=>[{...first,message:'old'},second])),[{kind:'max',bound:'0',message:'max'},{kind:'min',bound:'3',message:'min'},{kind:'length',bound:'2',message:'exact'}]];
const rows=[],expected=[];
for(const checks of chains)for(const mode of ['borrowed','checked','union','intersection'])for(const value of values){
 const element=mode!=='borrowed';
 let schema=statics.array(element?statics.string.trim():statics.unknown);
 for(const {kind,bound,message}of checks)schema=kind==='nonempty'?schema.nonempty(message):schema[kind](Number(bound),message);
 if(mode==='union')schema=schema.or(statics.unknown);
 if(mode==='intersection')schema=schema.and(schema);
 rows.push({checks,mode,element,...(value===undefined?{}:{value})});
 expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));
}
if(process.env.FORKLAUNCH_ARRAY_ORACLE_ONLY!=='1'){
 const run=spawnSync('cargo',['run','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_array_checks_parity'],{input:rows.map(value=>JSON.stringify(value)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});
 assert.equal(run.status,0,run.stderr);const actual=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(actual.length,rows.length);
 for(let i=0;i<rows.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(rows[i]));
}
console.log(`PASS ${rows.length} actual-source${process.env.FORKLAUNCH_ARRAY_ORACLE_ONLY==='1'?'':'/native'} numeric array constraints/messages, overwrite/order/element/union/intersection comparisons`);
