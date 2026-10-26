import assert from 'node:assert/strict';
import {createRequire,Module} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {Readable} from 'node:stream';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const compile=source=>ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
const encryptionSource=path.join(root,'framework/core/src/persistence/fieldEncryptor.ts');
const loaded=new Module(encryptionSource);loaded.filename=encryptionSource;loaded._compile(compile(await readFile(encryptionSource,'utf8')),encryptionSource);
const {FieldEncryptor}=loaded.exports;
const objects=new Map(),requests=[];
const bucket='json-object-fixture',prefix='tenant/owned/';
const server=createServer(async(request,response)=>{
 try{
  const url=new URL(request.url,'http://fixture');
  requests.push({method:request.method,path:url.pathname,authorization:request.headers.authorization});
  assert.ok(url.pathname.startsWith(`/${bucket}/${prefix}`));
  if(url.searchParams.has('tagging')){response.setHeader('content-type','application/xml');response.end('<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><TagSet/></Tagging>');return;}
  if(request.method==='PUT'){
   const chunks=[];for await(const chunk of request)chunks.push(chunk);
   objects.set(url.pathname,{body:Buffer.concat(chunks),contentType:request.headers['content-type']});response.end();return;
  }
  if(request.method==='DELETE'){objects.delete(url.pathname);response.statusCode=204;response.end();return;}
  const object=objects.get(url.pathname);
  if(!object){response.statusCode=404;response.setHeader('content-type','application/xml');response.end('<Error><Code>NoSuchKey</Code></Error>');return;}
  response.setHeader('content-type',object.contentType);response.setHeader('content-length',object.body.length);response.end(object.body);
 }catch(error){response.statusCode=500;response.end('fixture request failed');server.fixtureError=error;}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}`;
class PutObjectCommand {constructor(input){this.input=input;}}
class GetObjectCommand {constructor(input){this.input=input;}}
class DeleteObjectCommand {constructor(input){this.input=input;}}
class DeleteObjectsCommand {constructor(input){this.input=input;}}
// Execute actual TS object-store methods over their injected SDK client boundary.
const client={async send(command){
 const {Bucket,Key,Body,ContentType,Delete}=command.input;
 if(command instanceof DeleteObjectsCommand){await Promise.all(Delete.Objects.map(({Key})=>client.send(new DeleteObjectCommand({Bucket,Key}))));return {};}
 if(command instanceof DeleteObjectCommand){await fetch(`${base}/${Bucket}/${Key}`,{method:'DELETE'});return {};}
 const response=await fetch(`${base}/${Bucket}/${Key}`,command instanceof PutObjectCommand?{method:'PUT',body:Body,headers:{'content-type':ContentType}}:{});
 if(!response.ok)throw new Error('Fixture object missing');
 return command instanceof PutObjectCommand?{}:{Body:{transformToString:()=>response.text()}};
}};
const exports={};
vm.runInNewContext(compile(await readFile(path.join(root,'framework/infrastructure/S3/index.ts'),'utf8')),{exports,Date,Number,require(name){
 if(name==='@aws-sdk/client-s3')return {PutObjectCommand,GetObjectCommand,DeleteObjectCommand,DeleteObjectsCommand};
 if(['@aws-sdk/s3-presigned-post','@aws-sdk/lib-storage','@aws-sdk/s3-request-presigner'].includes(name))return {};
 if(name==='stream')return {Readable};
 throw new Error(`Unexpected runtime import ${name}`);
}});
const master='native-json-current-fixture-key',previous=['native-json-previous-fixture-key'],tenant='tenant-a';
const env={...process.env,S3_URL:base,S3_BUCKET:bucket,S3_PREFIX:prefix,S3_REGION:'us-east-1',S3_ACCESS_KEY_ID:'owned-json-fixture',S3_SECRET_ACCESS_KEY:'owned-json-fixture-secret',AWS_EC2_METADATA_DISABLED:'true'};
async function native(operations,options={}){
 const child=spawn('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-infrastructure-s3','--features','aws-sdk,encryption','--example','json_object_parity'],{cwd:root,env,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify({master,previous,tenant,operations,...options}));
 assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);return JSON.parse(output);
}
const actual=(encryptor)=>new exports.S3ObjectStore({}, {bucket,client,prefix,createBucketIfMissing:false},{},{encryptor});
try{
 for(const format of ['v2','v3']){
  const store=actual(new FieldEncryptor(master,{previousKeys:previous,format})),compliance={tenantId:tenant};
  const value={email:'private@example.test',unicode:'秘密🙂',nested:[false,0,null]};
  const tsKey=`${format}-ts`,rustKey=`${format}-rust`;
  await store.putObject({key:tsKey,...value},compliance);
  assert.deepEqual(await native([{op:'get',key:tsKey}],{format}),[{found:true,value,contentType:'application/json'}]);
  assert.deepEqual(await native([{op:'put',key:rustKey,value}],{format}),[null]);
  assert.deepEqual(JSON.parse(JSON.stringify(await store.readObject(rustKey,compliance))),value);
  const bytes=objects.get(`/${bucket}/${prefix}${rustKey}`).body.toString();
  // Native writers always seal randomized, stamped v4 envelopes (JsonRecordCodec::encode).
  assert.ok(bytes.startsWith('v4:'),bytes);assert.ok(!bytes.includes(value.email));
  assert.ok((await native([{op:'get',key:rustKey}],{tenant:'wrong'}))[0].error);
  assert.equal((await native([{op:'get',key:rustKey}],{tenant:null}))[0].error,'record_tenant_required');
  await assert.rejects(store.readObject(rustKey,{tenantId:'wrong'}));
  for(const marker of ['$serde_json::private::Number','$serde_json::private::RawValue']) {
   const value={[marker]:'123',nested:[{[marker]:{literal:true}}]},label=marker.endsWith('Number')?'number':'raw',tsKey=`${format}-${label}-ts`,rustKey=`${format}-${label}-rust`;
   await store.putObject({key:tsKey,...value},compliance);
   assert.deepEqual((await native([{op:'get',key:tsKey}],{format}))[0].value,value);
   await native([{op:'put',key:rustKey,value}],{format});
   assert.deepEqual(JSON.parse(JSON.stringify(await store.readObject(rustKey,compliance))),value);
  }
  const old=actual(new FieldEncryptor(previous[0],{format}));
  await old.putObject({key:`${format}-old`,...value},compliance);
  await store.putObject({key:`${format}-plain`,...value});
  assert.deepEqual(await native([{op:'get',key:`${format}-old`},{op:'get',key:`${format}-plain`},{op:'get',key:'missing'}],{format}),[{found:true,value,contentType:'application/json'},{found:true,value,contentType:'application/json'},{found:false}]);

  const batch=[{key:`${format}-batch-a`,order:1},{key:`${format}-batch-b`,order:2}];
  await store.streamUploadBatchObjects(batch,compliance);
  assert.deepEqual(await native([{op:'getBatch',keys:[batch[1].key,'missing',batch[0].key]}],{format}),[[{found:true,value:{order:2},contentType:'application/json'},{found:false},{found:true,value:{order:1},contentType:'application/json'}]]);
  const falsy=[false,0,null];const nativeKeys=falsy.map((_,index)=>`${format}-falsy-${index}`);
  await native([{op:'putBatch',objects:nativeKeys.map((key,index)=>({key,value:falsy[index]}))}],{format});
  assert.deepEqual(JSON.parse(JSON.stringify(await store.readBatchObjects(nativeKeys,compliance))),falsy);
  assert.deepEqual(await native([{op:'deleteBatch',keys:[...nativeKeys,'missing']}],{format}),[3]);
  await store.deleteBatchObjects(batch.map(object=>object.key));
  assert.deepEqual(await native([{op:'getBatch',keys:batch.map(object=>object.key)},{op:'putBatch',objects:[]},{op:'getBatch',keys:[]},{op:'deleteBatch',keys:[]}],{format}),[[{found:false},{found:false}],null,[],0]);
  await store.streamUploadObject({key:`${format}-stream-alias`,order:3},compliance);
  assert.deepEqual(await native([{op:'delete',key:`${format}-stream-alias`},{op:'delete',key:`${format}-stream-alias`}],{format}),[true,false]);
  objects.set(`/${bucket}/${prefix}corrupt`,{body:Buffer.from('v2:bad:bad:bad'),contentType:'application/json'});
  assert.ok((await native([{op:'get',key:'corrupt'}]))[0].error);
 }
 assert.ifError(server.fixtureError);
 const signed=requests.filter(request=>request.authorization);assert.ok(signed.length>10);
 for(const request of signed)assert.match(request.authorization,/Credential=owned-json-fixture\//);
 console.log('PASS actual TypeScript/native S3 JSON wire exchange over injected TS client and signed native SDK requests: batches, JSON stream aliases, falsy values, deletes, v2/v3 both directions, prefix, content type, rotation, plaintext migration, misses and tenant/corruption rejection');
}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
