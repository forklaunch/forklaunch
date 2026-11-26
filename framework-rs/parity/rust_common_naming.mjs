// Compare every published naming/string-hash export with the actual TS source.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualValidator(root);
const reference={...load(path.join(root,'framework/common/src/camelCase.ts')),
 ...load(path.join(root,'framework/common/src/hashString.ts')),
 ...load(path.join(root,'framework/common/src/slashManipulation.ts')),
 ...load(path.join(root,'framework/common/src/openApiCompliantPath.ts'))};
const cases=['/:id','/:name.:ext','/prefix:id','/:café','/:用户','/:','::id','/:id-name','/:123_ABC','/http://localhost:8000/:param','/{existing}/:next', '/:'+'a'.repeat(5000),'/:_/:9', '/:İ/:K/:ß', '/','//','///','////','users','users/','users//','/users///','///users//','//a//b///','','HTTP-Response_id','API-Key','123invalid','123-ABC','123','!!!',' --_./:',
 'foo!bar','foo@bar','foo\tbar','foo\nbar','foo\u00a0bar','foo?bar','foo#bar','foo[bar]',
 '$money','_private','FOO__bar','École.İSTANBUL','Kelvin/ßeta','İstanbul','ßeta','𐐨word','𐐀word','😀hello',
 'a\0b','Σςσ','a'.repeat(5000),String.fromCodePoint(0x10ffff)];
for(const prefix of ['', '/', '//', '///'])
for(const body of ['', 'users', 'é/用户'])
for(const suffix of ['', '/', '//', '///', '/\n', '/\r', '/\u2028', '/\u2029'])
 cases.push(prefix+body+suffix);
const atoms=['a','Z','1','0',' ','-','_', '.', '/', ':','!', '$','\t','é','İ','K','ß','😀','𐐨'];
for(const left of atoms)for(const right of atoms)cases.push(`${left}${right}HTTP`);
let seed=0x12345678;
for(let n=0;n<200;n++){
 let text='';for(let i=0;i<30;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;text+=atoms[seed%atoms.length];}
 cases.push(text);
}
const expected=cases.map(input=>Object.fromEntries(Object.entries(reference).map(([name,fn])=>[name,fn(input)])));
const native=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-common','--example','naming_parity'],{
 input:cases.map(input=>Buffer.from(input).toString('hex')).join('\n')+'\n',encoding:'utf8',maxBuffer:16*1024*1024,timeout:180000,
 env:{...process.env,CARGO_INCREMENTAL:'0'},
});
assert.equal(native.status,0,native.stderr);
const results=native.stdout.trim().split('\n').map(line=>JSON.parse(line));
assert.equal(results.length,cases.length);
for(let index=0;index<cases.length;index++)assert.deepEqual(results[index],JSON.parse(JSON.stringify(expected[index])),JSON.stringify(cases[index]));
console.log(`PASS ${cases.length} inputs across ${Object.keys(reference).length} actual TypeScript naming/hash/path exports (${cases.length*Object.keys(reference).length} comparisons)`);
