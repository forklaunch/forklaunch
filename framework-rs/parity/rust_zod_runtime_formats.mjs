import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
assert.equal(process.versions.node,'24.2.0','Unicode16 semantic fixture');
const {validator,statics}=actualValidator(root);
const jwt=(header)=>Buffer.from(JSON.stringify(header),'latin1').toString('base64url')+'.e30.';
const specs=['uuid','email','cuid','cuid2','ulid','nanoid','base64','base64url','date','duration','emoji'].map(kind=>({kind}));
for(const precision of [null,0,1,3,1000])specs.push({kind:'time',precision});
for(const precision of [null,0,3])for(const local of [false,true])for(const offset of [false,true])specs.push({kind:'datetime',precision,local,offset});
for(const kind of ['ip','cidr'])for(const version of [null,'v4','v6'])specs.push({kind,version});
for(const algorithm of [undefined,'HS256','none','','é','\0'])specs.push({kind:'jwt',...(algorithm===undefined?{}:{algorithm})});
const values=[undefined,null,false,0,[],{},'', 'a','abc',' a ','\n','😀','1','🇺🇸','\u200d','水','é',
 'FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF',"a'b@b-.co",'a%b@b.co','c12345678','C😀😀😀😀','123','abc-123','00000000000000000000000000','_'.repeat(21),
 'YWJj','YQ==','YR==','__8=','--__','A===','2024-02-29','1900-02-29','0000-02-29','2024-01-01\n','P1D','PT1.5S','P','12:34:56','12:34:56.123','12:34:56.'+'1'.repeat(1000),
 '2024-02-29T12:34:56Z','2024-02-29T12:34:56.123+99:99','2024-02-29T12:34:56','2024-02-29t12:34:56z',
 '127.0.0.1','01.2.3.4','::1','::ffff:192.168.01.01','fe80::1%eth0','127.0.0.1/32','::1/128','::1/129',
 jwt({alg:'HS256'}),jwt({alg:'none'}),jwt({alg:'HS256',typ:'JWT'}),jwt({alg:'HS256',typ:'bad'}),jwt({alg:'é'}),jwt({alg:''}),jwt({alg:null})];
const rows=[];
function schema(checks){let result=statics.string;for(const {kind,message,...options} of checks){
 if(kind==='trim'){result=result.trim();continue;}
 if(kind==='date'){result=result.date(message);continue;}
 const opts={...options,...(message===undefined?{}:{message})};
 if(opts.version===null)delete opts.version;
 if(kind==='jwt'&&Object.hasOwn(opts,'algorithm')){opts.alg=opts.algorithm;delete opts.algorithm;}
 result=result[kind](opts);
}return result;}
for(const spec of specs)for(const message of [undefined,'','custom\n"é\0'])for(const order of ['plain','before','after']){
 const check={...spec,...(message===undefined?{}:{message})};
 const checks=order==='before'?[{kind:'trim'},check]:order==='after'?[check,{kind:'trim'}]:[check];
 const source=schema(checks);
 for(const value of values)for(const shape of ['direct','object','array']){
  if(value===undefined&&shape==='array')continue; // JSON null is not an undefined array slot.
  const input=shape==='object'?{value}:shape==='array'?[value]:value;
  const wrapped=shape==='object'?validator.schemify({value:source}):shape==='array'?validator.array(source):source;
  rows.push({input:{checks,shape,...(input===undefined?{}:{value:input})},expected:validator.parse(wrapped,input)});
 }
}
// Ordered conjunction accumulates all checks; transforms do not undo prior failures.
for(const checks of [[{kind:'email'},{kind:'uuid'}],[{kind:'date'},{kind:'duration'}],[{kind:'jwt',algorithm:'HS256'},{kind:'trim'},{kind:'jwt',algorithm:'none'}]]){
 const source=schema(checks);for(const value of values)rows.push({input:{checks,...(value===undefined?{}:{value})},expected:validator.parse(source,value)});
}
function run(command,args,options={}){const result=spawnSync(command,args,{cwd:root,env:{...process.env,CARGO_BUILD_JOBS:process.env.CARGO_BUILD_JOBS??'1',CARGO_INCREMENTAL:'0'},encoding:'utf8',maxBuffer:128*1024*1024,...options});assert.equal(result.status,0,result.stdout+'\n'+result.stderr);return result.stdout;}
if(process.env.FORKLAUNCH_RUNTIME_FORMATS_NATIVE==='1'){
 run('cargo',['build','--locked','--manifest-path','framework-rs/Cargo.toml','-p','forklaunch-validator','--example','zod_runtime_formats']);
 const executable=path.join(process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target'),'debug/examples/zod_runtime_formats');
 const actual=run(executable,[],{input:rows.map(row=>JSON.stringify(row.input)).join('\n')+'\n'}).trim().split('\n').map(JSON.parse);
 assert.equal(actual.length,rows.length);
 for(let index=0;index<rows.length;index++)assert.deepEqual(actual[index],JSON.parse(JSON.stringify(rows[index].expected)),JSON.stringify(rows[index].input));
 const invalid=[-1,1001,0.5].map(precision=>({checks:[{kind:'time',precision}],value:'12:00:00'})).concat([{checks:[{kind:'ip',version:'v5'}],value:'127.0.0.1'}]);
 for(const result of run(executable,[],{input:invalid.map(JSON.stringify).join('\n')+'\n'}).trim().split('\n').map(JSON.parse))assert.ok(result.unsupported);
}
console.log(`PASS ${rows.length} actual Zod ordered runtime format/diagnostic/composition observations${process.env.FORKLAUNCH_RUNTIME_FORMATS_NATIVE==='1'?' and native comparisons; 4 typed unsupported registrations':''}; finite Unicode JSON inputs only`);
