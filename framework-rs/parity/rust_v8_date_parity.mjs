// Native V8 string-date conformance. The runtime library never starts Node.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,readFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const temp=mkdtempSync(path.join(tmpdir(),'fl-v8-date-reference-'));
const target=process.env.CARGO_TARGET_DIR??path.join(root,'blueprint-rs/target');
const env={...process.env,CARGO_TARGET_DIR:target,CARGO_BUILD_JOBS:process.env.CARGO_BUILD_JOBS??'1',CARGO_INCREMENTAL:'0'};
function run(command,args,options={}){const result=spawnSync(command,args,{encoding:'utf8',maxBuffer:64<<20,timeout:300000,...options});assert.equal(result.status,0,`${command} ${args.join(' ')}\n${result.stdout}\n${result.stderr}`);return result.stdout;}
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i).toString(16).padStart(4,'0')).join('');
const reference=(rows,TZ)=>JSON.parse(run(process.execPath,['--input-type=module','-e',`let text='';for await(const part of process.stdin)text+=part;console.log(JSON.stringify(JSON.parse(text).map(value=>{const time=new Date(value).getTime();return Number.isNaN(time)?null:time})));`],{input:JSON.stringify(rows),env:{...process.env,TZ}}));
try{
 const module=path.join(root,'framework-rs/crates/common/src/javascript_date');
 const provenance=JSON.parse(readFileSync(path.join(module,'provenance.json'),'utf8'));
 for(const [relative,expected]of Object.entries(provenance.retainedFiles))assert.equal(createHash('sha256').update(readFileSync(path.join(module,relative))).digest('hex'),expected,relative);
 const referencePath=path.join(temp,'reference.json');
 run(process.execPath,[path.join(root,'framework-rs/parity/fixtures/rust_v8_date_grammar_reference.mjs')],{env:{...process.env,FORKLAUNCH_DATE_REFERENCE_OUTPUT:referencePath}});
 const original=JSON.parse(readFileSync(referencePath,'utf8'));
 assert.equal(process.versions.tz,'2025b','Date oracle requires the declared pinned2025b rules');
 const rows=new Set([...original.rows,"2026-01-01T00:00:00.00000000123456789Z","Jan 1 2026 00:00:00.00000000123456789 GMT"]);
 for(const year of ['0000','0001','0049','0050','0099','0100','2026','-000000','+000000','+275760','-271821'])for(const month of ['0','00','01','1','12','13'])for(const day of ['00','01','1','28','29','30','31','32'])rows.add(`${year}-${month}-${day}`);
 for(const white of ['\u0085','\u2028','\u2029','\ufeff','\u00a0','\t','\r','\n'])for(const base of ['2026-01-01','Jan 1 2026','1/2/2026']){rows.add(white+base);rows.add(base+white);}
 for(let length=0;length<35;length++)rows.add('2026-01-01T00:00:00.'+'1234567890'.repeat(4).slice(0,length)+'Z');
 for(const offset of ['999999999','596523','596524','1193046','123456789','000000000999999999'])for(const sign of ['+','-'])rows.add(`Jan 1 2026 GMT${sign}${offset}:59`);
 for(const surrogate of ['\ud800','\udfff'])for(const base of ['2026-01-01','Jan 1 2026','1/2/2026']){rows.add(surrogate+base);rows.add(base+surrogate);}
 let seed=0x5eed1234;const tokens=['2026','01','15','Jan','GMT','UTC','T','Z','+','-',':','.', '/', ' ','(x)','\u0000','AM','PM','99','0000','1234567890'];
 for(let i=0;i<2000;i++){let text='';for(let j=0;j<1+i%8;j++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;text+=tokens[seed%tokens.length];}rows.add(text);}
 const samples=[...rows];
 run('cargo',['build','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-common','--features','javascript-date','--example','javascript_date_parity'],{env});
 const executable=path.join(target,'debug/examples/javascript_date_parity'+(process.platform==='win32'?'.exe':''));
 let count=0;
 function compare(input,zone,host=false){const expected=reference(input,zone);const output=run(executable,[host?'--host':zone],{input:input.map(units).join('\n')+'\n',env:{...process.env,TZ:zone}}).trimEnd().split('\n');assert.equal(output.length,input.length);for(let i=0;i<input.length;i++)assert.equal(output[i],expected[i]===null?'null':String(expected[i]),JSON.stringify({input:input[i],zone,host}));count+=input.length;}
 for(const zone of Object.keys(original.results))compare(samples,zone);
 const named=[...readFileSync(path.join(module,'tzdb/tzname.rs'),'utf8').matchAll(/^\s*\(r"([^"]+)",/gm)].map(m=>m[1]);
 const dates=['1900-01-01T12:00','1940-07-01T12:00','1970-01-01T12:00','2026-01-01T12:00','2026-07-01T12:00','+275760-09-12T00:00','-271821-04-21T00:00'];
 for(const zone of named)compare(dates,zone);
 if(process.platform!=='win32')for(const zone of ['','UTC','GMT','GMT0','GMT+0','GMT-0','GMT+8','GMT+08','GMT-8','UTC+8','GMT+8:30','Etc/GMT+8','Etc/GMT-8','America/Los_Angeles',':America/Los_Angeles','america/los_angeles','AMERICA/LOS_ANGELES',':GMT+8','No/SuchZone','America/Invalid','EST','EDT','PST','PDT','CET','EET','MET','WET','EST5EDT','CST6CDT','MST7MDT','PST8PDT','Foo3','Foo-3','AEST-10AEDT,M10.1.0,M4.1.0/3','Asia/Kathmandu','Asia/Calcutta','Asia/Kolkata','US/Pacific','posix/America/Los_Angeles','right/America/Los_Angeles',':posix/America/Los_Angeles','/usr/share/zoneinfo/America/Los_Angeles','Factory','localtime','Europe/Kyiv','Europe/Kiev','Zulu','Universal','UCT','Etc/Unknown'])compare(dates,zone,true);
 console.log(`PASS ${count} actual Node24.2/V8 date observations: grammar/UTF16/fullTimeClip,598 pinned zones, Unix host fallback; licenses/data hashes verified. No runtime Node subprocess.`);
}finally{rmSync(temp,{recursive:true,force:true});}
