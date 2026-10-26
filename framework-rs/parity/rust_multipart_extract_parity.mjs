import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {Readable} from 'node:stream';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const dependencies=process.env.FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES;
assert.ok(dependencies,'Set FORKLAUNCH_TEST_CANONICAL_DEPENDENCIES to the installed canonical-iam fixture');
const require=createRequire(join(dependencies,'package.json'));
const ts=require('typescript');
process.env.TSC_BIN??=require.resolve('typescript/bin/tsc');
process.env.FORKLAUNCH_TEST_VALIDATOR_DEPENDENCIES??=dependencies;
const {validator}=actualValidator(root);
function load(path,imports={}){
 const exports={};
 const source=ts.transpileModule(readFileSync(join(root,path),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
 vm.runInNewContext(source,{exports,Buffer,require(name){assert.ok(name in imports,`unexpected import ${name}`);return imports[name];}});
 return exports;
}
const discriminator=load('framework/core/src/http/router/discriminateBody.ts');
const common=load('framework/common/src/guards/isNever.ts');
const {contentParse}=load('framework/express/src/middleware/content.parse.middleware.ts',{
 '@forklaunch/common':common,'@forklaunch/core/http':discriminator,express:require('express'),busboy:require('busboy')
});
function wire(parts){return Buffer.concat(parts.flatMap(([name,file,bytes,mime])=>[
 Buffer.from(`--boundary\r\nContent-Disposition: form-data; name="${name}"${file===null?'':`; filename="${file}"`}\r\n${mime?`Content-Type: ${mime}\r\n`:''}\r\n`),Buffer.from(bytes),Buffer.from('\r\n')
]).concat(Buffer.from('--boundary--\r\n')));}
async function parse(bytes){
 const req=Readable.from([bytes]);req.headers={'content-type':'multipart/form-data; boundary=boundary'};
 req.contractDetails={body:{multipartForm:{}}};req.schemaValidator={};
 await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('parser deadline')),5000);contentParse()(req,{},error=>{clearTimeout(timer);error?reject(error):resolve();});});
 return Object.fromEntries(Object.entries(req.body).map(([key,value])=>[key,Buffer.isBuffer(value)?[...value]:value]));
}
const cases=[
 [['label',null,'hello 水'],['file','a.bin',[0,255,128]]],
 [['label',null,'first'],['label',null,'last'],['file','empty.bin',[]]],
 [['label',null,'ok'],['file','a.bin',[1]],['file','b.bin',[2,3]]],
 [['label',null,'line\r\nbreak'],['file','../name.bin',[0]]],
 [['label',null,'ok'],['file',null,[0,255],'application/octet-stream']],
 [['label',null,'ok'],['file','',[],'application/octet-stream']],
 [['label',null,'ok'],['file','text.txt','hello 水','text/plain']],
];
// Member order: insertion order, except JavaScript array-index names enumerate first.
const orderCase=[['b',null,'1'],['label',null,'ok'],['a',null,'2'],['10',null,'3'],['2',null,'4'],['file','x.bin',[1]]];
// Handler-visible member order (JavaScript Object.keys), as object key paths depth first.
function keyPaths(value,prefix='',out=[]){
 if(Array.isArray(value))value.forEach((item,index)=>keyPaths(item,`${prefix}/${index}`,out));
 else if(value&&typeof value==='object'&&!Buffer.isBuffer(value))for(const key of Object.keys(value)){const path=`${prefix}/${key}`;out.push(path);keyPaths(value[key],path,out);}
 return out;
}
const temporary=mkdtempSync(join(tmpdir(),'fl-multipart-extract-'));
try{
 const fixtures=[];
 for(const parts of cases){const bytes=wire(parts);fixtures.push({bytes:[...bytes],expected:await parse(bytes)});}
 const orderedBytes=wire(orderCase);
 const exactFixtures=[...fixtures,{bytes:[...orderedBytes],expected:await parse(orderedBytes)}];
 const versions=exactFixtures.map(({bytes,expected})=>({kind:'multipart',includeKeyOrder:true,versions:[{name:'fallback',schema:{type:'object'}},{name:'exact',schema:{const:expected}}],cases:[bytes]}));
 const extraBytes=wire([['label',null,'ok'],['file',null,'text file fallback']]);
 const fileFixtures=[...fixtures,{bytes:[...extraBytes],expected:await parse(extraBytes)}];
 for(const {bytes,expected} of fileFixtures) {
  assert.equal(validator.file.safeParse(Array.isArray(expected.file)?Buffer.from(expected.file):expected.file).success,true);
  const file=validator.openapi(validator.file);
  for(const dialect of ['http://json-schema.org/draft-04/schema#','http://json-schema.org/draft-06/schema#','http://json-schema.org/draft-07/schema#','https://json-schema.org/draft/2019-09/schema','https://json-schema.org/draft/2020-12/schema']) {
   for(const storage of ['direct','definitions','default']) {
    const schema={$schema:dialect,type:'object',required:['label','file'],properties:{label:{type:'string'},file:storage==='direct'?file:{$ref:`#/${storage}/file`}},...(storage==='direct'?{}:{[storage]:{file}})};
    versions.push({kind:'multipart',versions:[{name:'file',schema}],cases:[bytes]});
   }
   const legacy=dialect.includes('draft-0'),id=dialect.includes('draft-04')?'id':'$id',defs=legacy?'definitions':'$defs';
   for(const fragment of ['', '#bytes', ...(legacy?['#/bytes']:[])]) {
    const reference=`file.json${fragment}`;
    const target={...file,[id]:legacy?reference:'file.json',...(!legacy&&fragment?{$anchor:'bytes'}:{})};
    const schema={$schema:dialect,[id]:'https://example.invalid/root.json',type:'object',required:['file'],properties:{file:{$ref:reference}},[defs]:{File:target}};
    versions.push({kind:'multipart',versions:[{name:'file',schema}],cases:[bytes]});
   }
   const literal={$schema:dialect,[id]:'https://example.invalid/literal.json',type:'object',required:['file'],properties:{file:{$ref:'#/default/file'}},default:{file}};
   versions.push({kind:'multipart',versions:[{name:'file',schema:literal}],cases:[bytes]});
  }
 }
 const selected=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','http','--example','version_selection'],{input:JSON.stringify(versions),encoding:'utf8',env:{...process.env,CARGO_INCREMENTAL:'0'},timeout:300000}));
 for(const [index,result] of selected.entries())assert.deepEqual(result.cases,index<exactFixtures.length?[{version:'exact',matched:['fallback','exact'],status:200,bodyKeys:keyPaths(exactFixtures[index].expected)}]:[{version:'file',matched:['file'],status:200}],`multipart schema ${index}`);
 console.log(`PASS ${versions.length} actual multipart parser/file-validator/native version comparisons, including member order`);
 const path=join(temporary,'cases.json');writeFileSync(path,JSON.stringify(fixtures));
 execFileSync('cargo',['test','--manifest-path',join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-axum','--features','multipart','multipart_matches_typescript_parser','--','--nocapture'],{env:{...process.env,CARGO_INCREMENTAL:'0',FORKLAUNCH_TEST_MULTIPART_CASES:path},stdio:'inherit',timeout:300000});
 console.log(`PASS ${cases.length} actual TypeScript content-parser / native typed multipart comparisons`);
}finally{rmSync(temporary,{recursive:true,force:true});}
