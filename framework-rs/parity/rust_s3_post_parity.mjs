import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath,mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {spawn} from 'node:child_process';
import {Readable} from 'node:stream';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
let captured;
const localRequire=name=>{
  if(name==='@aws-sdk/client-s3')return {S3Client:class{}};
  if(name==='@aws-sdk/s3-presigned-post')return {createPresignedPost:async(_client,options)=>{captured=options;return {url:'https://owned.example/bucket',fields:options.Fields};}};
  if(name==='@aws-sdk/lib-storage'||name==='@aws-sdk/s3-request-presigner')return {};
  if(name==='stream')return {Readable};
  throw new Error(`Unexpected runtime import ${name}`);
};
const exports={};
const compiled=ts.transpileModule(await readFile(path.join(root,'framework/infrastructure/S3/index.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
vm.runInNewContext(compiled,{exports,require:localRequire,Date,Number},{filename:'actual-typescript-s3.ts'});
const cases=[
  {key:'plain',prefix:'',maxBytes:1,contentType:'text/plain'},
  {key:'/browser/é file.txt',prefix:'/tenant/owned/',maxBytes:1024,contentType:'text/plain',expiresIn:60},
  {key:'folder/file.bin',prefix:'tenant//owned///',maxBytes:16777216,contentType:'application/octet-stream',expiresIn:900},
  {key:'fractional',prefix:'tenant',maxBytes:32,contentType:'text/plain',expiresIn:1.9},
  {key:'invalid',prefix:'tenant',maxBytes:0,contentType:'text/plain'},
  {key:'invalid',prefix:'tenant',maxBytes:1.5,contentType:'text/plain'},
  {key:'invalid',prefix:'tenant',maxBytes:32,contentType:'text/plain',expiresIn:0},
  {key:'invalid',prefix:'tenant',maxBytes:32,contentType:'text/plain',expiresIn:901},
];
const fixtures=[];
for(const input of cases){
  captured=undefined;
  const store=new exports.S3ObjectStore({}, {bucket:'post-parity',client:{},prefix:input.prefix}, {},{encryptor:{}});
  try{
    const grant=await store.presignUpload(input.key,input);
    assert.equal(grant.key,input.key);
    assert.ok(grant.expiresAt instanceof Date);
    fixtures.push({...input,accepted:true,wireKey:captured.Key,conditions:captured.Conditions,fields:captured.Fields,seconds:captured.Expires});
  }catch(error){ fixtures.push({...input,accepted:false}); }
}
assert.equal(fixtures.filter(fixture=>fixture.accepted).length,4);
const directory=await mkdtemp(path.join(tmpdir(),'forklaunch-s3-post-parity-'));
try{
  const file=path.join(directory,'fixtures.json');
  await writeFile(file,JSON.stringify(fixtures));
  const child=spawn('cargo',['test','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-infrastructure-s3','--all-features','post_policy_matches_actual_typescript','--','--ignored'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_S3_POST_FIXTURES:file},stdio:'inherit'});
  assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);
}finally{await rm(directory,{recursive:true,force:true});}
console.log('PASS actual TypeScript S3 upload policy bounds, fields, prefixes, types and lifetimes match Rust');
