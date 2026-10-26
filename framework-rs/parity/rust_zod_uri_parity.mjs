import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
assert.equal(process.versions.node,'24.2.0','URI compatibility oracle requires Node24.2.0');
assert.equal(process.versions.ada,'3.2.4','URI compatibility oracle requires Ada3.2.4');
const require=createRequire(path.join(process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES,'package.json'));
const punycode=require('punycode/');
const {validator,statics}=actualValidator(root);
const fixture=name=>JSON.parse(readFileSync(path.join(root,'framework-rs/parity/fixtures',name),'utf8'));
const wpt=fixture('rust_zod_uri_wpt.json'),provenance=fixture('rust_zod_uri_provenance.json');
const values=new Set(wpt.inputs);
for(const scheme of ['http','https','ftp','ws','wss','file','custom','mailto','data','javascript','blob'])for(const host of ['example.com','localhost','127.1','0x7f.1','0177.1','4294967295','4294967296','[::1]','[::ffff:192.168.0.1]','[fe80::1%25eth0]','faß.de','☃.net','例え.テスト','a\u200db.com','\u00ad','xn--','xn--a','%FF','%00','-bad.test','bad-.test','a_b.test','a..b','a。b',''])for(const suffix of ['',':65535',':65536','/a b?x=1#z','/../%2e/%zz','\\foo'])values.add(`${scheme}://${host}${suffix}`);
function hostForms(cp){const ch=String.fromCodePoint(cp),host=`a${ch}b.test`;values.add('http://'+host);values.add('http://'+encodeURIComponent(host));values.add('http://'+punycode.toASCII(host));}
for(let cp=0;cp<0x10000;cp++)if(cp<0xd800||cp>0xdfff){hostForms(cp);if(cp<256){const char=String.fromCodePoint(cp);for(const value of [char+'http://example.com'+char,'http://exam'+char+'ple.com','custom:a'+char+'b'])values.add(value);}}
// All UTF-8 widths, sampled supplementary blocks and their encoded host forms.
for(let cp=0x10000;cp<=0x10ffff;cp+=257)hostForms(cp);
for(const cp of [0x1f980,0x1fae0,0x1e4d0,0x10ffff])hostForms(cp);
const cases=[],expected=[];
for(const value of values){assert.ok(value.isWellFormed());const parsed=validator.parse(statics.uri,value);let accepts;try{new URL(value);accepts=true}catch{accepts=false}assert.equal(parsed.ok,accepts);if(parsed.ok)assert.equal(parsed.value,value);cases.push({format:'uri',order:'plain',value});expected.push(JSON.parse(JSON.stringify(parsed)));}
const admissionCount=cases.length;
const edges=[undefined,null,0,false,[],{},'', 'invalid',' https://example.com/a b ','http://a\u061db.test','http://a\u0c5cb.test','http://xn--ab-hsd.test','http://a%D8%9Db.test','file:\\\\localhost//','mailto:a@b','custom:a\0b','\u2028http://a.test','https://a.test/z'];
for(const order of ['plain','trimFirst','mixed'])for(const message of [undefined,'','custom\n💡\0'])for(const value of edges){let schema=statics.string;if(order==='trimFirst')schema=schema.trim();schema=schema.url(message===undefined?undefined:{message});if(order==='mixed')schema=schema.trim().endsWith('z');cases.push({format:'uri',order,...(message===undefined?{}:{message}),...(value===undefined?{}:{value})});expected.push(JSON.parse(JSON.stringify(validator.parse(schema,value))));}
const compositions=[],compositionExpected=[];
for(const value of edges)for(const variant of ['object','union','array']){
 const leaf={kind:'string',checks:[{name:'url'}]};
 let spec,schema,input;
 if(variant==='object'){spec={kind:'object',fields:[['uri',{kind:'optional',inner:leaf}]]};schema=validator.schemify({uri:statics.uri.optional()});input=value===undefined?{}:{uri:value};}
 else if(variant==='union'){spec={kind:'union',branches:[leaf,{kind:'unknown'}]};schema=validator.union([statics.uri,statics.unknown]);input=value;}
 else {spec={kind:'array',inner:leaf};schema=validator.array(statics.uri);input=value===undefined?[]:[value];}
 compositions.push({schema:spec,...(input===undefined?{}:{value:input})});
 const parsed=validator.parse(schema,input);compositionExpected.push(parsed.ok?(parsed.value===undefined?{ok:true,absent:true}:{ok:true,absent:false,value:tag(parsed.value)}):JSON.parse(JSON.stringify(parsed)));
}
function tag(value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {$number:b.toString('hex')}}if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)]));return value;}
function cargo(args,input){const r=spawnSync('cargo',args,{input,encoding:'utf8',timeout:300000,maxBuffer:128*1024*1024,env:{...process.env,CARGO_INCREMENTAL:'0'}});assert.equal(r.status,0,r.stderr);return r.stdout;}
const manifest=path.join(root,'framework-rs/Cargo.toml');
if(process.env.FORKLAUNCH_NUMBER_ORACLE_ONLY==='1')console.log(`PASS ${admissionCount} actual Node/Ada factory URI admission observations +${cases.length-admissionCount} ordered-message cases +${compositions.length} compositions`);
else {
 const output=cargo(['run','--quiet','--manifest-path',manifest,'-p','forklaunch-validator','--features','zod-uri','--example','zod_string_formats_parity'],cases.map(v=>JSON.stringify(v)).join('\n')+'\n').trim().split('\n').map(JSON.parse);
 assert.equal(output.length,cases.length);for(let i=0;i<cases.length;i++)assert.deepEqual(output[i],expected[i],JSON.stringify(cases[i]));
 const composed=cargo(['run','--quiet','--manifest-path',manifest,'-p','forklaunch-validator','--features','zod-uri','--example','zod_json_parity'],compositions.map(v=>JSON.stringify(v)).join('\n')+'\n').trim().split('\n').map(JSON.parse);
 assert.deepEqual(composed,compositionExpected);
 // Offline metadata resolves every locked package, including other targets'
 // dependencies a host build never downloads; fetch the locked set first.
 cargo(['fetch','--locked','--manifest-path',manifest]);
 const metadata=JSON.parse(cargo(['metadata','--format-version','1','--locked','--offline','--manifest-path',manifest,'--features','forklaunch-validator/zod-uri']));
 const lock=readFileSync(path.join(root,'framework-rs/Cargo.lock'),'utf8').split('[[package]]').find(block=>block.includes('name = \"ada-url\"')&&block.includes('version = \"3.2.4\"'));assert.ok(lock?.includes(provenance.crate_sha256),'pinned official crate archive checksum');
 const dependency=metadata.packages.find(p=>p.name==='ada-url'&&p.version===provenance.version);assert.ok(dependency);
 for(const [name,source] of Object.entries(provenance.sources)){const relative=name.replace('crate-deps-','deps/');assert.equal(createHash('sha256').update(readFileSync(path.join(path.dirname(dependency.manifest_path),relative))).digest('hex'),source.sha256);}
 const graph=cargo(['tree','--manifest-path',manifest,'-p','forklaunch-validator','--no-default-features','-e','normal,build']);assert.ok(!graph.includes('ada-url'),'default graph must not compile Ada/C++');
 console.log(`PASS ${admissionCount} exact Node24.2.0/Ada3.2.4 URI admission comparisons +${cases.length-admissionCount} ordered-message cases +${compositions.length} compositions; pinned source hashes and default dependency graph verified`);
}
