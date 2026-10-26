import {gzipSync,deflateSync,brotliCompressSync,gunzipSync,inflateSync,brotliDecompressSync} from 'node:zlib';
// Compare actual Express content parsing with native version body matching.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {Readable} from 'node:stream';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES');
const require=createRequire(join(dependencies,'package.json'));
const ts=require('typescript');
function load(path,imports={}) {
 const exports={};
 const source=ts.transpileModule(readFileSync(join(root,path),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
 vm.runInNewContext(source,{exports,Buffer,require(name){assert.ok(name in imports,`unexpected import ${name}`);return imports[name];}});
 return exports;
}
const {contentParse}=load('framework/express/src/middleware/content.parse.middleware.ts',{
 '@forklaunch/common':load('framework/common/src/guards/isNever.ts'),
 '@forklaunch/core/http':load('framework/core/src/http/router/discriminateBody.ts'),
 express:require('express'),busboy:require('busboy')
});
// Handler-visible member order (JavaScript Object.keys), as object key paths depth first.
function keyPaths(value,prefix='',out=[]){
 if(Array.isArray(value))value.forEach((item,index)=>keyPaths(item,`${prefix}/${index}`,out));
 else if(value&&typeof value==='object'&&!Buffer.isBuffer(value))for(const key of Object.keys(value)){const path=`${prefix}/${key}`;out.push(path);keyPaths(value[key],path,out);}
 return out;
}
const cases = [
 ['json',Buffer.from('{}')], ['json',Buffer.from('[]')], ['json',Buffer.alloc(0)],
 ['json',Buffer.from(' null')], ['json',Buffer.from('true')], ['json',Buffer.from('42')], ['json',Buffer.from('"text"')],
 ['json',Buffer.from(' \t\r\n{"value":1}')], ['json',Buffer.from(' ')], ['json',Buffer.from('{')],
 ['json',Buffer.from('\ufeff{"value":"水"}')], ['json',Buffer.from('\ufeff')], ['json',Buffer.from('\ufeff\ufeff{}')],
 ['json',Buffer.from([123,34,120,34,58,34,255,34,125])], ['json',Buffer.from([123,34,120,34,58,34,0xe6,0xb0,34,125])],
 ['text',Buffer.from('水')], ['text',Buffer.alloc(0)], ['text',Buffer.from('\ufefftext')], ['text',Buffer.from('\ufeff\ufefftext')],
 ['text',Buffer.from([255])], ['text',Buffer.from([0xe6,0xb0])], ['text',Buffer.from([0xc0,0xaf])], ['text',Buffer.from([0xed,0xa0,0x80])],
];
// Member order: insertion order, except JavaScript array-index keys enumerate first.
cases.push(['json',Buffer.from('{"b":1,"a":{"d":1,"c":2},"10":3,"2":4,"b2":[{"z":1,"y":2}]}')],['json',Buffer.from('{"b":1,"a":2,"b":3}')],['form',Buffer.from('b=1&a=2&10=3&2=4&x[d]=1&x[c]=2&x[1]=3')]);
const iconv=require('iconv-lite');
for(const charset of ['utf-8','utf-16le','utf-16be','utf-16','utf-32le','utf-32be','utf-32']) {
 for(const value of ['', 'abc 水 🦀', '\ufefftext']) {
  cases.push(['text',iconv.encode(value,charset),charset]);
  cases.push(['json',iconv.encode(JSON.stringify({value}),charset),charset]);
 }
}
for(const charset of ['iso-8859-1','latin1','windows-1252','ascii','us-ascii','shift_jis','gb18030','big5','koi8-r']) {
 for(const bytes of [Buffer.from([0,65,127,128,160,233,255]),iconv.encode('café 水',charset)]) cases.push(['text',bytes,charset]);
 cases.push(['json',Buffer.from('{}'),charset]);
}
for(const charset of ['utf-8','iso-8859-1']) for(const value of ['value=café','value=%E9','value=%E9%zz%20','value=%E6%B0%B4','a[%E9]=%80','value=%26%3D','value=%u1234','value=%2','value=&#233;','utf8=%E2%9C%93&value=%E9']) cases.push(['form',iconv.encode(value,charset),charset]);
for(const kind of ['text','json','form']) cases.push([kind,Buffer.from('a'), 'no-such-charset']);
cases.push(['json',Buffer.from('{}'),'utf8'],['form',Buffer.from('a=b'),'latin1'],['form',Buffer.from([97,61,255]),'utf-8']);
for(const kind of ['json','text']) for(const [encoding,charset] of [['utf-16be','utf-16'],['utf-32be','utf-32'],['utf-16le','"UTF-16LE"']]) {
 cases.push([kind,iconv.encode(kind==='json'?' {"value":"水"}':'abc 水',encoding),charset]);
}
for(const charset of ['utf-16le','utf-16be','utf-32le','utf-32be']) cases.push(['text',Buffer.concat([iconv.encode('abc',charset),Buffer.from([1])]),charset]);
for(const value of ['null','true','42','"text"','', ' ', '{']) cases.push(['json',Buffer.from(value),undefined,{json:{strict:false}}]);
cases.push(['json',iconv.encode('{"value":"水"}','utf-16le'),undefined,{json:{defaultCharset:'utf-16le'}}]);
for(const charset of [undefined,'utf-8']) cases.push(['text',Buffer.from([233]),charset,{text:{defaultCharset:'iso-8859-1'}}]);
for(const options of [{extended:false},{depth:0},{depth:1},{depth:2},{parameterLimit:2}]) {
 for(const value of ['a[b]=x','a[b][c]=x','a[]=x&a[]=y','a=x&b=y&c=z','__proto__=x&a[__proto__]=y','1[]=a&1=b&1[]=c']) cases.push(['form',Buffer.from(value),undefined,{urlencoded:options}]);
}
for(const charset of [undefined,'utf-8']) cases.push(['form',Buffer.from('value=%E9'),charset,{urlencoded:{defaultCharset:'iso-8859-1'}}]);
const compressors={gzip:gzipSync,deflate:deflateSync,br:brotliCompressSync};
const decompressors={gzip:gunzipSync,deflate:inflateSync,br:brotliDecompressSync};
for(const kind of ['json','text','form','file']) {
 const parser=kind==='form'?'urlencoded':kind==='file'?'raw':kind;
 const bytes=Buffer.from(kind==='json'?'{"value":"水"}':kind==='form'?'value=%E6%B0%B4':'water 水');
 for(const limit of [0,bytes.length-1,bytes.length]) cases.push([kind,bytes,undefined,{[parser]:{limit}}]);
 cases.push([kind,Buffer.alloc(0),undefined,{[parser]:{limit:0}}]);
 cases.push([kind,Buffer.alloc(102401,97)]);
 for(const [encoding,compress] of Object.entries(compressors)) {
  const compressed=compress(bytes);
  cases.push([kind,compressed,undefined,undefined,encoding]);
  cases.push([kind,compressed,undefined,{[parser]:{inflate:false}},encoding]);
  cases.push([kind,compressed,undefined,{[parser]:{limit:bytes.length-1}},encoding]);
  cases.push([kind,compressed,undefined,{[parser]:{limit:bytes.length}},encoding]);
  cases.push([kind,compressed.subarray(0,compressed.length-2),undefined,undefined,encoding]);
  cases.push([kind,Buffer.alloc(0),undefined,undefined,encoding]);
 }
 cases.push([kind,bytes,undefined,undefined,'unsupported']);
}
cases.push(['text',Buffer.concat([gzipSync(Buffer.from('one')),gzipSync(Buffer.from('two'))]),undefined,undefined,'gzip']);
for(const charset of ['utf-8','iso-8859-1']) for(const charsetSentinel of [false,true]) for(const interpretNumericEntities of [false,true]) {
 for(const value of [
  'utf8=%E2%9C%93&value=%E6%B0%B4','utf8=%26%2310003%3B&value=%E9',
  'value=%E9&utf8=%26%2310003%3B','utf8=other&value=%E9',
  'utf8=%e2%9c%93&value=%E9','utf8=%26%2310003%3B&utf8=%E2%9C%93&value=%E9',
  'utf8=%E2%9C%93&utf8=%26%2310003%3B&value=%E9',
  'value=%26%239786%3B','value=%26%2355357%3B%26%2356832%3B',
  'value=%26%2365536%3B','value=%26%239007199254740993%3B',
  `value=%26%23${'9'.repeat(400)}%3B`,
  'value=%26%23x%3B%26%23%3B%26%2342','value[]=%26%2365%3B&value[]=%26%2366%3B',
  '%26%2365%3B=%26%2366%3B','utf8=','utf8=%26%2310003%3B&value=%26%239786%3B',
 ]) cases.push(['form',Buffer.from(value),charset,{urlencoded:{charsetSentinel,interpretNumericEntities}}]);
}
cases.push(['form',Buffer.from('utf8=%E2%9C%93&a=b&c=d'),undefined,{urlencoded:{charsetSentinel:true,parameterLimit:2}}]);
for(const kind of ['json','text','form','file']) {
 const parser=kind==='form'?'urlencoded':kind==='file'?'raw':kind;
 const bytes=Buffer.from(kind==='json'?' {"value":"水"}':kind==='form'?'value=%E6%B0%B4':'water 水');
 for(const encoding of [undefined,'gzip','deflate','br']) for(const reject of [false,true]) {
  const wire=encoding?compressors[encoding](bytes):bytes;
  const verify={reject,expected:{bytes:[...bytes],charset:kind==='file'?null:'utf-8'}};
  cases.push([kind,wire,undefined,undefined,encoding,verify]);
  cases.push([kind,wire,undefined,{[parser]:{limit:bytes.length-1}},encoding,verify]);
 }
 for(const reject of [false,true]) {
  const bytes=Buffer.alloc(0);
  cases.push([kind,bytes,undefined,undefined,undefined,{reject,expected:{bytes:[],charset:kind==='file'?null:'utf-8'}}]);
 }
}
for(const value of ['{','true']) for(const reject of [false,true]) {
 const bytes=Buffer.from(value);
 cases.push(['json',bytes,undefined,undefined,undefined,{reject,expected:{bytes:[...bytes],charset:'utf-8'}}]);
}
for(const kind of ['json','text']) for(const reject of [false,true]) {
 const bytes=Buffer.from(kind==='json'?'\ufeff{"value":"水"}':'\ufeff水','utf16le');
 cases.push([kind,gzipSync(bytes),'utf-16le',undefined,'gzip',{reject,expected:{bytes:[...bytes],charset:'utf-16le'}}]);
}
for(const value of ['{"a":2,"nested":{"drop":3,"value":" a "},"items":[1,2]}','{}','[]','42','" hi "','null','{"reject":1}','{"__proto__":{"value":2},"constructor":2}','{"2":2,"10":10,"1":1,"01":3}','','\ufeff',' ','{']) {
 for(const encoding of [undefined,'gzip']) {
  const bytes=Buffer.from(value);
  cases.push(['json',encoding?gzipSync(bytes):bytes,undefined,{json:{strict:false}},encoding,undefined,true]);
 }
}
for(const value of ['{"z":1,"2":2,"1":1,"10":10,"nested":{"drop":0,"q":3},"array":[4,5],"z":9}','{"4294967295":1,"03":2,"4294967294":3,"1":4,"a":5}','[1,{"z":2,"a":3},4]']) {
 cases.push(['json',Buffer.from(value),undefined,undefined,undefined,undefined,'trace']);
}
const fixtures=[],expected=[];
for(const [kind,bytes,charset,parserOptions,contentEncoding,verify,reviver] of cases) {
 const req=Readable.from([bytes]);
 const contentType=(kind==='json'?'application/json':kind==='form'?'application/x-www-form-urlencoded':kind==='file'?'application/octet-stream':'text/plain')+(charset?`; charset=${charset}`:'');
 req.headers={'content-type':contentType,'content-length':String(bytes.length),...(contentEncoding?{'content-encoding':contentEncoding}:{})};
 req.contractDetails={versions:{first:{body:{[kind==='form'?'urlEncodedForm':kind]:{}}}}}; req.schemaValidator={};
 let failure;
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>reject(Error('parser deadline')),5000);
  const parser=kind==='form'?'urlencoded':kind==='file'?'raw':kind;
  const options=verify?{...parserOptions,[parser]:{...parserOptions?.[parser],verify(_req,_res,buf,encoding){
   assert.deepEqual({bytes:[...buf],charset:encoding},verify.expected);
   if(verify.reject)throw Error('private verification detail');
  }}}:parserOptions;
  let reviverCount=0;
  const configured=reviver?{...options,json:{...options?.json,reviver(key,value){
   const ordinal=++reviverCount;
   if(key==='reject')throw Error('private reviver detail');
   if(key==='drop')return undefined;
   if(typeof value==='number')return reviver==='trace'?ordinal:value*2;
   if(typeof value==='string')return value.trim().toUpperCase();
   return value;
  }}}:options;
  contentParse(configured)(req,{},error=>{clearTimeout(timer);error?reject(error):resolve();});
 }).catch(error=>{failure=error;});
 const versions=[{name:'fallback',schema:true}];
 if(failure) expected.push([{status:failure.status}]);
 else {
  assert.deepEqual(req._rawBody,contentEncoding?decompressors[contentEncoding](bytes):bytes);
  const body=Buffer.isBuffer(req.body)?[...req.body]:req.body;
  versions.push({name:'exact',schema:{...(kind==='file'?{type:'array'}:{}),const:body}});
  expected.push([{version:'exact',matched:['fallback','exact'],status:200,body,bodyKeys:keyPaths(req.body)}]);
 }
 fixtures.push({kind,contentType,contentEncoding,parserOptions,verify,reviver,wireBytes:true,includeBody:true,includeKeyOrder:true,versions,cases:[[...bytes]]});
}
const native=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_selection'],{input:JSON.stringify(fixtures),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},timeout:300000,maxBuffer:32*1024*1024}));
for(const [i,result] of native.entries()) assert.deepEqual(result.cases,expected[i],`${cases[i][0]} ${cases[i][2]} ${cases[i][4]} ${cases[i][1].subarray(0,64).toString('hex')}`);
console.log(`PASS ${fixtures.length} actual TypeScript/native JSON/text/form/file wire-parser comparisons: parser options, charsets, compression, bounded decoded bytes and member order`);
