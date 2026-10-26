import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
assert.equal(process.versions.node, '24.2.0', 'Regex oracle pins Node 24.2.0 / Unicode 16');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const {validator,statics}=actualValidator(root);
// Exponential-backtracking patterns the kernel must agree on; kept as data so
// static analysis does not mistake the deliberate fixtures for live regexes.
const backtracking=JSON.parse(readFileSync(path.join(root,'framework-rs/parity/fixtures/rust_zod_regex_backtracking.json'),'utf8'));
const patterns=[...['','.', '^.$','^..$','^a$','a','a+','a*','a?','a{2,4}','(a|ab)b','(a)?b\\1','(a)\\1','(?<x>a)\\k<x>','(?=a)a','(?!b)a','(?<=a)b','(?<!a)b','^.$','[a-z]','[A-Z]','\\w','\\W','\\b.\\b','\\s','\\S','\\d','\\D','[^]','[😀]','😀','\\ud800','\\udc00','\\u{1F600}','\\p{Letter}','\\p{Script=Kawi}','\\p{Script=Todhri}','\\p{Script=Garay}','\\p{RGI_Emoji}','[\\p{ASCII}&&\\p{Letter}]','[\\p{ASCII}--[A-Z]]','[a\\q{bc}]','k','s','ß','Σ','µ','[\\s-1]','\\8','[]','[^a]','a$','.$','^','(a|(b))\\2','(?=(a+))a*b\\1'],...["[\\q{}]", "[\\q{|a}]", "[\\q{a|}]", "[\\q{a||bc}]", "[\\q{ab|a}--\\q{ab}]", "[\\q{ab|a}&&\\q{a}]", "[^\\q{a}]", "[^\\q{}]", "[\\p{ASCII}a-z]", "[[\\p{ASCII}]&&[a-z]]", "[^\\p{ASCII}]", "[\\P{ASCII}--[a-z]]", "[\\P{Letter}]", "\\P{Lowercase_Letter}", "[\\p{Lowercase_Letter}--[a-z]]", "(?<x>a)|(?<x>b)", "(?<x>a)(?<x>b)", "(?<=([ab]+)([bc]+))$", "(a|(b))\\2c", backtracking[0], backtracking[1], backtracking[2], "(?=(a|aa))\\1b", "[a&&b]", "[a--b]", "[a-z&&b]", "[\\q{ss|\u00df}]"]];
const flags=['','i','m','s','u','v','iu','iv','g','y','gy','d','dgimsuy','gimsvy'];
const values=['','a','A','ab','aa','ba','b','aba','abbaa','\n','\r','\r\n','\u2028','\u2029','a\n','a\r\n','0',' ','\t','\v','\f','\u0085','\ufeff','K','ſ','ß','ẞ','σ','ς','Σ','µ','μ','😀','\ud800','\udc00','😀a','a😀','é','水','👨‍👩‍👦','\u{11F02}','\u{105C0}','\u{10D50}'];
for(let code=0;code<128;code++)values.push(String.fromCharCode(code));
const units=value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));const rows=[],expected=[],meta=[];let invalid=0;
for(const pattern of patterns)for(const flag of flags){let regex;try{regex=new RegExp(pattern,flag);}catch{invalid++;continue;}const schema=statics.string.regex(regex);for(const value of values){rows.push({pattern:units(regex.source),flags:flag,text:units(value)});expected.push(validator.parse(schema,value).ok);meta.push({pattern,flags:flag,text:value});}}

const result=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-common','--features','zod-regex','--example','zod_regex_parity'],{input:rows.map(row=>[row.pattern.join(','),row.flags,row.text.join(',')].join('\t')).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:64<<20});
assert.equal(result.status,0,result.stderr);
const actual=result.stdout.trim().split('\n');
assert.equal(actual.length,rows.length);
for(let i=0;i<actual.length;i++)assert.equal(actual[i],String(expected[i]),JSON.stringify(meta[i]));
console.log(`PASS ${rows.length} actual ForkLaunch/Node/native bounded regex comparisons; ${invalid} invalid constructor combinations characterized separately`);

const grammarPatterns=[...patterns,'(','[','[z-a]','a{2,1}','(?<x>a)(?<x>b)','(?<x>a)|(?<x>b)','(?<1>a)','(?<x>a)\\k<z>','\\p{Invalid}','\\p{sc=Kawi}','\\p{Script=kawi}','[a&&]','[a--]','[a&&b--c]','[^\\q{ab}]','[^\\q{}]','[^\\p{RGI_Emoji}]','\\P{RGI_Emoji}'];
const admissions=[];
for(const pattern of grammarPatterns)for(const flag of [...flags,'gg','uv','z']) {
 let valid=true;try{new RegExp(pattern,flag);}catch{valid=false;}
 admissions.push({pattern,flags:flag,valid});
}
const registration=spawnSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-common','--features','zod-regex','--example','zod_regex_parity'],{input:admissions.map(row=>[units(row.pattern).join(','),row.flags,''].join('\t')).join('\n')+'\n',encoding:'utf8',timeout:300000,maxBuffer:16<<20});
assert.equal(registration.status,0,registration.stderr);
const admitted=registration.stdout.trim().split('\n');assert.equal(admitted.length,admissions.length);
for(let i=0;i<admitted.length;i++) {
 assert.ok(!admitted[i].startsWith('resource:'),JSON.stringify(admissions[i]));
 assert.equal(admitted[i]==='true'||admitted[i]==='false',admissions[i].valid,JSON.stringify({row:admissions[i],native:admitted[i]}));
}
console.log(`PASS ${admissions.length} Node/native constructor admission comparisons including invalid grammar and flags`);
