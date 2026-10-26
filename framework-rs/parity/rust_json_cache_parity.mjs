import assert from 'node:assert/strict';
import {createRequire,Module} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import {spawn,execFileSync} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import {redisTransport} from './fixtures/redis_resp.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const compile=source=>ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
const encryptionSource=path.join(root,'framework/core/src/persistence/fieldEncryptor.ts');
const loaded=new Module(encryptionSource);loaded.filename=encryptionSource;loaded._compile(compile(await readFile(encryptionSource,'utf8')),encryptionSource);
const {FieldEncryptor}=loaded.exports;
const url=process.env.FORKLAUNCH_TEST_REDIS_URL??'redis://127.0.0.1:32770/12';
const transport=redisTransport(url),keys=[];
const client={
 exists:key=>transport.command('EXISTS',key),
 keys:pattern=>transport.command('KEYS',pattern),
 del:key=>transport.command('DEL',key),
 set:(key,value,options)=>transport.command('SET',key,value,'PX',options.PX),
 lPush:(key,value)=>transport.command('LPUSH',key,value),
 rPop:key=>transport.command('RPOP',key),
 lRange:(key,start,end)=>transport.command('LRANGE',key,start,end),
 multi(){const commands=[];const multi={exists(key){commands.push(['EXISTS',key]);return multi;},del(key){commands.push(['DEL',key]);return multi;},set(key,value,options){commands.push(['SET',key,value,'PX',options.PX]);return multi;},lPush(key,value){commands.push(['LPUSH',key,value]);return multi;},rPop(key){commands.push(['RPOP',key]);return multi;},get(key){commands.push(['GET',key]);return multi;},ttl(key){commands.push(['TTL',key]);return multi;},exec:()=>transport.transaction(commands)};return multi;},
};
const common={};
for(const helper of ['safeParse','safeStringify']){const file=path.join(root,`framework/common/src/${helper}.ts`);const module=new Module(file);module.filename=file;module._compile(compile(await readFile(file,'utf8')),file);Object.assign(common,module.exports);}
const exports={};
vm.runInNewContext(compile(await readFile(path.join(root,'framework/infrastructure/redis/index.ts'),'utf8')),{exports,Buffer,require(name){
 if(name==='@forklaunch/common')return common;
 if(name==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{logging:false}})};
 if(name==='redis')return {createClient:()=>client};
 throw new Error(`Unexpected runtime import ${name}`);
}});
const master='native-json-current-fixture-key',previous=['native-json-previous-fixture-key'],tenant='tenant-a';
const key=()=>{const value=`fl-json-parity:${randomUUID()}`;keys.push(value);return value;};
let nativeExecutable;
async function buildNative(){
 const manifest=path.join(root,'framework-rs/Cargo.toml');
 const child=spawn('cargo',['build','--quiet','--manifest-path',manifest,'-p','forklaunch-infrastructure-redis','--features','redis-client,encryption','--example','json_cache_parity'],{cwd:root,env:process.env,stdio:'inherit'});
 await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',code=>code===0?resolve():reject(new Error(`Native fixture build failed: ${code}`)));});
 const metadata=JSON.parse(execFileSync('cargo',['metadata','--manifest-path',manifest,'--no-deps','--format-version','1'],{cwd:root,env:process.env,encoding:'utf8'}));
 return path.join(metadata.target_directory,'debug/examples/json_cache_parity');
}
async function native(operations,options={}){
 const binary=await (nativeExecutable??=buildNative());
 const child=spawn(binary,[],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_REDIS_URL:url},stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify({master,previous,tenant,operations,typescriptSafe:true,...options}));
 assert.equal(await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);}),0);return JSON.parse(output);
}
try{
 for(const format of ['v2','v3']){
  const actual=new exports.RedisTtlCache(60000,{}, {url},{},{encryptor:new FieldEncryptor(master,{previousKeys:previous,format})});
  const compliance={tenantId:tenant};
  for(const value of [{email:'private@example.test',unicode:'秘密🙂',nested:[null,false,0]},...['$serde_json::private::Number','$serde_json::private::RawValue'].map(key=>({[key]:'123',nested:[{[key]:{literal:true}}]})),false,0,null,[], 'v2:ordinary-json-string']){
   const tsKey=key(),rustKey=key();
   await actual.putRecord({key:tsKey,value},compliance);
   assert.deepEqual(await native([{op:'get',key:tsKey}],{format}),[{found:true,value}]);
   assert.deepEqual(await native([{op:'put',key:rustKey,value}],{format}),[null]);
   const record=await actual.readRecord(rustKey,compliance);
   assert.deepEqual(record.value,value);assert.ok(record.ttlMilliseconds>0);
   // Native writers always seal randomized, stamped v4 envelopes (JsonRecordCodec::encode).
   const wire=await transport.command('GET',rustKey);assert.ok(wire.startsWith('v4:'),wire);
   assert.deepEqual(common.safeParse(new FieldEncryptor(master,{previousKeys:previous}).decrypt(wire,tenant)),value);
   assert.ok((await native([{op:'get',key:rustKey}],{tenant:'wrong-tenant'}))[0].error);
   assert.equal((await native([{op:'get',key:rustKey}],{tenant:null}))[0].error,'record_tenant_required');
   await assert.rejects(actual.readRecord(rustKey,{tenantId:'wrong-tenant'}));
  }
  for(const text of ['', 'plain text', 'null', 'false', '0', '123', '01', '"quoted"', '[1,false]', '{"ok":true}', '{bad', '用户\n😀']) {
   const tsKey=key(),rustKey=key(),expected=JSON.parse(JSON.stringify(common.safeParse(text)));
   await actual.putRecord({key:tsKey,value:text},compliance);
   assert.deepEqual(await native([{op:'get',key:tsKey}],{format}),[{found:true,value:expected}]);
   await native([{op:'put',key:rustKey,value:text}],{format});
   assert.deepEqual(JSON.parse(JSON.stringify((await actual.readRecord(rustKey,compliance)).value)),expected);
   assert.equal(new FieldEncryptor(master,{previousKeys:previous}).decrypt(await transport.command('GET',rustKey),tenant),text);
  }
  const queue=key();
  await actual.enqueueBatchRecords(queue,[{order:1},{order:2}],compliance);
  assert.deepEqual(await native([{op:'queuePeekBatch',count:10},{op:'queueGet'},{op:'queuePut',value:{order:3}}],{queue,format}),[[{order:1},{order:2}],{found:true,value:{order:1}},null]);
  assert.deepEqual(JSON.parse(JSON.stringify(await actual.peekQueueRecords(queue,10,compliance))),[{order:2},{order:3}]);
  assert.deepEqual(await actual.dequeueRecord(queue,compliance),{order:2});
  assert.ok((await native([{op:'queuePeek'}],{queue,tenant:'wrong'}))[0].error);
  assert.deepEqual(await native([{op:'queueGetBatch',count:10}],{queue,format}),[[{order:3}]]);
  await native([{op:'queuePutBatch',values:[{order:4},{order:5}]}],{queue,format});
  assert.deepEqual(JSON.parse(JSON.stringify(await actual.dequeueBatchRecords(queue,2,compliance))),[{order:4},{order:5}]);
  await native([{op:'queuePutBatch',values:[false,0,null]}],{queue,format});
  assert.deepEqual(await native([{op:'queuePeekBatch',count:10},{op:'queueGetBatch',count:10},{op:'queueGet'}],{queue,format}),[[false,0,null],[false,0,null],{found:false,value:null}]);
  const mixed=[false,0,null,'',true,1,-1,'0','false',' ',[],{},'[]','text',{order:1},{'$serde_json::private::Number':'123'},{'$serde_json::private::RawValue':{literal:true}}];
  for(const count of [0,1,2,5,25]) {
   const tsQueue=key(),rustQueue=key();
   await actual.enqueueBatchRecords(tsQueue,mixed,compliance);
   await native([{op:'queuePutBatch',values:mixed}],{queue:rustQueue,format});
   const expectedPeek=JSON.parse(JSON.stringify(await actual.peekQueueRecords(tsQueue,count,compliance)));
   assert.deepEqual(await native([{op:'queuePeekBatchTs',count}],{queue:rustQueue,format}),[expectedPeek]);
   assert.ok((await native([{op:'queuePeekBatchTs',count}],{queue:rustQueue,format,tenant:'wrong'}))[0].error);
   const expectedPop=JSON.parse(JSON.stringify(await actual.dequeueBatchRecords(tsQueue,count,compliance)));
   assert.deepEqual(await native([{op:'queueGetBatchTs',count}],{queue:rustQueue,format}),[expectedPop]);
   assert.deepEqual(await transport.command('LLEN',rustQueue),await transport.command('LLEN',tsQueue));
   assert.deepEqual(await native([{op:'queuePeekBatchTs',count:0}],{queue:rustQueue,format}),[JSON.parse(JSON.stringify(await actual.peekQueueRecords(tsQueue,0,compliance)))]);
  }
  const badQueue=key();await transport.command('LPUSH',badQueue,'v2:bad:bad:bad');
  assert.ok((await native([{op:'queuePeekBatchTs',count:0}],{queue:badQueue,format}))[0].error);
  assert.equal(await transport.command('LLEN',badQueue),1);
  const timed=key(),persistent=key();
  const [written,timedRecord]=await native([{op:'put',key:timed,value:false,ttl:1250},{op:'getWithTtl',key:timed}],{format});
  assert.equal(written,null);assert.equal(timedRecord.value,false);assert.ok(timedRecord.ttl>1000&&timedRecord.ttl<=1250);
  await transport.command('SET',persistent,JSON.stringify({persistent:true}));
  assert.deepEqual(await native([{op:'getWithTtl',key:persistent}],{format}),[{key:persistent,value:{persistent:true},ttl:null}]);

  const first=key(),second=key(),absent=key();
  await actual.putBatchRecords([{key:first,value:{batch:1},ttlMilliseconds:6000},{key:second,value:{batch:2},ttlMilliseconds:12000}],compliance);
  const [tsRecords]=await native([{op:'readRecords',keys:[second,absent,first]}],{format});
  assert.deepEqual(tsRecords.map(record=>record?.value??null),[{batch:2},null,{batch:1}]);
  assert.ok(tsRecords[0].ttl>tsRecords[2].ttl);assert.ok(tsRecords[0].ttl<=12000&&tsRecords[2].ttl<=6000);
  await native([{op:'putRecords',records:[{key:first,value:{native:1},ttl:6000},{key:second,value:{native:2},ttl:12000}]}],{format});
  const tsRead=await actual.readBatchRecords([second,first],compliance);
  assert.deepEqual(JSON.parse(JSON.stringify(tsRead.map(record=>record.value))),[{native:2},{native:1}]);
  assert.ok(tsRead[0].ttlMilliseconds>tsRead[1].ttlMilliseconds);
  const rejected=key();
  assert.equal((await native([{op:'putRecords',records:[{key:rejected,value:1,ttl:6000},{key:key(),value:2,ttl:0}]}]))[0].error,'redis_cache_invalid_ttl');
  assert.equal(await transport.command('GET',rejected),null);
  assert.equal((await native([{op:'putRecords',records:[{key:rejected,value:1},{key:' ',value:2}]}]))[0].error,'redis_cache_invalid_key');
  assert.equal(await transport.command('GET',rejected),null);
  assert.deepEqual(await native([{op:'putRecords',records:[]},{op:'readRecords',keys:[]}]),[null,[]]);
  const literal=key()+'[?]*';keys.push(literal);
  await actual.putRecord({key:literal,value:{literal:true}},compliance);
  const [matched]=await native([{op:'prefix',key:literal}],{format});assert.equal(matched.length,1);assert.equal(matched[0].key,literal);assert.deepEqual(matched[0].value,{literal:true});
  const rotated=key(),plain=key(),missing=key(),corrupt=key();
  const old=new exports.RedisTtlCache(60000,{}, {url},{},{encryptor:new FieldEncryptor(previous[0],{format})});
  await old.putRecord({key:rotated,value:{rotation:'old-key'}},compliance);
  await actual.putRecord({key:plain,value:{legacy:'plaintext'}});
  assert.deepEqual(await native([{op:'batch',keys:[rotated,missing,plain]}],{format}),[[{rotation:'old-key'},null,{legacy:'plaintext'}]]);
  await transport.command('SET',corrupt,'v2:bad:bad:bad','PX',60000);
  assert.ok((await native([{op:'get',key:corrupt}]))[0].error);
  const list=key(),raw=key(),absentPeek=key();
  await transport.command('LPUSH',list,'not-json');
  await transport.command('SET',raw,'not-json');
  const peekKeys=[corrupt,list,absentPeek,raw,rotated,raw];
  const expectedPeeks=Array.from(await actual.peekBatchRecords(peekKeys));
  assert.deepEqual(expectedPeeks,[true,true,false,true,true,true]);
  assert.deepEqual(await native([{op:'get',key:raw}],{tenant:null}),[{found:true,value:'not-json'}]);
  assert.equal((await native([{op:'get',key:raw}],{tenant:null,typescriptSafe:false}))[0].error,'record_json_invalid');
  assert.deepEqual(await native([{op:'peekBatch',keys:peekKeys},{op:'peek',key:list},{op:'peek',key:absentPeek},{op:'rawExists',key:corrupt},{op:'ttl'}],{tenant:null}),[expectedPeeks,true,false,true,actual.getTtlMilliseconds()]);
  assert.deepEqual(await native([{op:'peekBatch',keys:[]},{op:'deleteBatch',keys:[]}]),[[],0]);
  assert.equal((await native([{op:'peekBatch',keys:[raw,' ']}]))[0].error,'redis_cache_invalid_key');
  assert.equal((await native([{op:'deleteBatch',keys:[raw,' ']}]))[0].error,'redis_cache_invalid_key');
  assert.equal(await actual.peekRecord(raw),true);
  const [listed,prefixPeeks]=await native([{op:'keys',key:raw},{op:'peekPrefix',key:raw}],{tenant:null});
  assert.deepEqual(listed,await actual.listKeys(raw));
  assert.deepEqual(prefixPeeks,[[raw,true]]);
  assert.deepEqual(Array.from(await actual.peekBatchRecords(raw)),[true]);
  assert.deepEqual(await native([{op:'peekPrefix',key:literal}],{tenant:null}),[[[literal,true]]]);
  const globPrefix=key()+':';
  for(const suffix of ['a','b','ab','x','?','[a]']){const globKey=globPrefix+suffix;keys.push(globKey);await transport.command('SET',globKey,'opaque');}
  for(const suffix of ['','[ab]','?','a','[!x]','\\?','\\[a\\]']) {
   const pattern=globPrefix+suffix;
   assert.deepEqual((await native([{op:'matchingKeys',key:pattern+'*'}],{tenant:null}))[0],(await actual.listKeys(pattern)).sort());
  }

  assert.deepEqual(await native([{op:'deleteBatch',keys:[raw,list,corrupt,raw,absentPeek]},{op:'peekBatch',keys:[raw,list,corrupt]}],{tenant:null}),[3,[false,false,false]]);
  await actual.deleteBatchRecords([rotated,plain]);
  assert.deepEqual(await native([{op:'peekBatch',keys:[rotated,plain]}]),[[false,false]]);
  const expires=key();await native([{op:'put',key:expires,value:{expires:true},ttl:1}]);
  await new Promise(resolve=>setTimeout(resolve,10));
  assert.deepEqual(await native([{op:'get',key:expires},{op:'peek',key:expires}]),[{found:false,value:null},false]);
 }
 console.log('PASS actual TypeScript/native encrypted Redis cache/queue JSON: actual safeParse/safeStringify string semantics, strict JSON opt-out, per-record batch/millisecond/persistent TTL metadata, literal prefix scans, prevalidated writes/deletes, typed existence batches without decryption, opt-in falsy queue filtering and zero-count peeks, raw client access, FIFO/batches/peeks, v2/v3 both directions, rotation, plaintext migration, falsy values, missing/expired keys and wrong-tenant/corruption rejection');
}finally{for(const key of keys)await transport.command('DEL',key);}
