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
async function parse(bytes,delayed=false){
 const stream=delayed?(async function*(){for(let i=0;i<bytes.length;i+=1){yield bytes.subarray(i,i+1);await new Promise(resolve=>setImmediate(resolve));}})():[bytes];
 const req=Readable.from(stream);req.headers={'content-type':'multipart/form-data; boundary=boundary'};
 req.contractDetails={body:{multipartForm:{}}};req.schemaValidator={};
 await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error('parser deadline')),5000);contentParse()(req,{},error=>{clearTimeout(timer);error?reject(error):resolve();});});
 return Object.fromEntries(Object.entries(req.body).map(([key,value])=>[key,Buffer.isBuffer(value)?[...value]:value]));
}

const rows=[];
for(const charset of [undefined,'utf-8','latin1','windows-1252','ascii','utf-16le','base64','shift_jis','unknown-owned']){
 for(const bytes of [[],[0x80,0xff],[0x41,0,0x42],[0xef,0xbb,0xbf,0x41],[0xc3,0x28]]){
  const out=await parse(wire([['field',null,bytes,charset?'text/plain; charset='+charset:undefined]]));
  rows.push({charset:charset??'default',bytes,keys:Object.keys(out),value:out.field??null,undefined:out.field===undefined});
 }
}
for(const parts of [[['field','a.bin',[1]],['field',null,'text']],[['field',null,'text'],['field','a.bin',[1]]]]){
 for(const delayed of [false,true]) rows.push({collision:parts.map(p=>p[1]===null?'text':'file'),delayed,out:await parse(wire(parts),delayed)});
}
for(const row of rows){
 if(row.collision){assert.deepEqual(row.out.field,row.delayed&&row.collision[0]==='file'?'text':[1]);continue;}
 const data=Buffer.from(row.bytes);
 const enc={default:'utf8','utf-8':'utf8',latin1:'latin1','windows-1252':'latin1',ascii:'latin1','utf-16le':'utf16le',base64:'base64'}[row.charset];
 const expected=enc?data.toString(enc):data.length?undefined:'';
 assert.equal(row.undefined,expected===undefined);assert.deepEqual(row.value,expected??null);assert.deepEqual(row.keys,['field']);
}
if(process.env.FORKLAUNCH_MULTIPART_REFERENCE_OUTPUT)writeFileSync(process.env.FORKLAUNCH_MULTIPART_REFERENCE_OUTPUT,JSON.stringify(rows,null,2));
console.log('PASS '+rows.length+' actual multipart charset/chunk-order observations (native comparison pending)');
