import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {Readable} from 'node:stream';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import {actualValidator} from '../rust_actual_validator_fixture.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
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
async function parse(bytes,delayed=false,limits={}){
 const stream=delayed?(async function*(){for(let i=0;i<bytes.length;i+=1){yield bytes.subarray(i,i+1);await new Promise(resolve=>setImmediate(resolve));}})():[bytes];
 const req=Readable.from(stream);req.headers={'content-type':'multipart/form-data; boundary=boundary'};
 req.contractDetails={body:{multipartForm:{}}};req.schemaValidator={};
 await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('parser deadline')),5000);contentParse({busboy:{limits}})(req,{},error=>{clearTimeout(timer);error?reject(error):resolve();});});
 return Object.fromEntries(Object.entries(req.body).map(([key,value])=>[key,Buffer.isBuffer(value)?[...value]:value]));
}


const rows=[];
const parts=[['text',null,'éabcdef'],['file','one.bin',[0,1,2,3,4,5]],['other',null,'second'],['binary','two.bin',[6,7,8]],['text',null,'last']];
for(const key of ['fields','files','parts'])for(const limit of [0,1,2,3,4,5,-1,1.5,NaN,Infinity,-Infinity])for(const delayed of [false,true]){
 const out=await parse(wire(parts),delayed,{[key]:limit});rows.push({key,limit:String(limit),delayed,out});
}
for(const key of ['fieldSize','fileSize'])for(const limit of [0,1,2,3,4,5,6,Infinity])for(const delayed of [false,true]){
 const out=await parse(wire(parts),delayed,{[key]:limit});rows.push({key,limit:String(limit),delayed,out});
}
for (const row of rows) {
 const limit=Number(row.limit), expected={};let fields=0,files=0;
 for (const [index,[name,file,data]] of parts.entries()) {
  if (row.key==='parts' && index>=limit) continue;
  const binary=file!==null;
  if(binary){if(row.key==='files' && files===limit)continue;files++;}
  else{if(row.key==='fields' && fields===limit)continue;fields++;}
  const bytes=Buffer.from(data), value=(row.key===(binary?'fileSize':'fieldSize'))?bytes.subarray(0,limit):bytes;
  expected[name]=binary?[...value]:value.toString('utf8');
 }
 assert.deepEqual(row.out,expected,JSON.stringify(row));
}
const oversized=Buffer.alloc(1024*1024+1,97);
for(const configured of [false,true]){
 const out=await parse(wire([['field',null,oversized]]),false,configured?{fieldSize:Infinity}:{});
 const expected=configured?oversized.length:1024*1024;
 assert.equal(out.field.length,expected);
 rows.push({kind:'defaultFieldSize',configuredInfinity:configured,inputBytes:oversized.length,outputLength:out.field.length});
}
if(process.env.FORKLAUNCH_MULTIPART_LIMITS_OUTPUT)writeFileSync(process.env.FORKLAUNCH_MULTIPART_LIMITS_OUTPUT,JSON.stringify(rows,null,2));
console.log('PASS '+rows.length+' actual multipart count/integer-byte-limit cases; native option integration remains pending');
