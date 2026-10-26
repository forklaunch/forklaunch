import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
assert.equal(process.versions.node,'24.2.0');assert.equal(process.versions.unicode,'16.0');
const {validator,statics}=actualValidator(root);
const lower=statics.string.toLowerCase(),upper=statics.string.toUpperCase();
const native=process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY!=='1';
const target=process.env.CARGO_TARGET_DIR??path.join(root,'framework-rs/target');
const executable=path.join(target,'debug/examples/zod_case_parity');
if(native){const built=spawnSync('cargo',['build','--locked','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-validator','--example','zod_case_parity'],{encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},timeout:300000});assert.equal(built.status,0,built.stderr);}
let scalars=0;
if(native){
 const child=spawn(executable,[],{stdio:['pipe','pipe','pipe']});let stderr='';child.stderr.on('data',chunk=>stderr+=chunk);const exit=new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code));});
 const timeout=setTimeout(()=>child.kill('SIGKILL'),300000);child.stdin.end('{"scalars":true}\n');
 child.stdout.setEncoding('utf8');
 async function* jsonLines(stream){let pending='';for await(const chunk of stream){pending+=chunk;let end;while((end=pending.indexOf('\n'))!==-1){yield pending.slice(0,end);pending=pending.slice(end+1);}}assert.equal(pending,'');}
 // readline also splits U+2028/U+2029, which are valid unescaped JSON string content.
 try {let cp=0;for await(const line of jsonLines(child.stdout)){
  if(cp===0xd800)cp=0xe000;const row=JSON.parse(line);assert.equal(row[0],cp);const value=String.fromCodePoint(cp);
  assert.equal(row[1],validator.parse(lower,value).value,`lower U+${cp.toString(16)}`);assert.equal(row[2],validator.parse(upper,value).value,`upper U+${cp.toString(16)}`);cp++;scalars+=2;
 }assert.equal(cp,0x110000);assert.equal(await exit,0,stderr);}finally{clearTimeout(timeout);if(child.exitCode===null)child.kill('SIGKILL');}
}else{for(let cp=0;cp<=0x10ffff;cp++){if(cp>=0xd800&&cp<=0xdfff)continue;const value=String.fromCodePoint(cp);assert.equal(validator.parse(lower,value).value,value.toLowerCase());assert.equal(validator.parse(upper,value).value,value.toUpperCase());scalars+=2;}}
const rows=[],expected=[];
function add(value,checks){let schema=statics.string;for(const check of checks)schema=check.argument===undefined?schema[check.name]():schema[check.name](check.argument,check.message);rows.push({value,checks});expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));}
const pieces=['','A','1','Σ','σ','ς','\u0345','\u0301',"'",'\u200d','İ','ß','ﬃ','ᾈ','𐐀','\u1c89','\ua7cb','a\u0301','\u0345\u0301'];
for(const a of pieces)for(const b of pieces)for(const c of pieces)for(const name of ['toLowerCase','toUpperCase'])add(a+'Σ'+b+'Σ'+c,[{name}]);
const operations=[
 [{name:'toUpperCase'},{name:'length',argument:2}],
 [{name:'max',argument:1},{name:'toUpperCase'}],
 [{name:'toUpperCase'},{name:'max',argument:1,message:'exact\n\0😀'}],
 [{name:'toLowerCase'},{name:'length',argument:2}],
 [{name:'trim'},{name:'toUpperCase'},{name:'toLowerCase'},{name:'startsWith',argument:'s'}],
 [{name:'toUpperCase'},{name:'trim'},{name:'min',argument:1},{name:'endsWith',argument:'S'}],
 [{name:'toLowerCase'},{name:'toUpperCase'},{name:'toLowerCase'}],
];
for(const input of [undefined,null,false,0,[],{},'', ' ß ', 'ß','İ','ΟΣ','ΟΣΑ','ﬃ','ᾈ','𐐀','é','e\u0301','AΣ\u0345','AΣ\u0345A'])for(const checks of operations)add(input,checks);
for(const name of ['toLowerCase','toUpperCase'])for(const value of ['AΣ'+'\u0345'.repeat(100000),'AΣ'+'\u0345'.repeat(100000)+'A'])add(value,[{name}]);
if(native){const run=spawnSync(executable,[],{input:rows.map(row=>JSON.stringify(row)).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:32*1024*1024});assert.equal(run.status,0,run.stderr);const actual=run.stdout.trim().split('\n').map(line=>JSON.parse(line));assert.equal(actual.length,rows.length);for(let i=0;i<rows.length;i++)assert.deepEqual(actual[i],expected[i],JSON.stringify(rows[i]).slice(0,1000));}
console.log(`PASS ${scalars} scalar mappings and ${rows.length} context/ordered/long-run ${native?'actual-source/native':'actual-source'} comparisons; Unicode16, valid UTF-8 domain`);
