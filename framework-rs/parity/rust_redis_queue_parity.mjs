import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import {redisTransport} from './fixtures/redis_resp.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const url=process.env.FORKLAUNCH_TEST_REDIS_URL??'redis://127.0.0.1:32770/12';
const transport=redisTransport(url),keys=[];
const client={
 lPush:(key,value)=>transport.command('LPUSH',key,value),
 rPop:key=>transport.command('RPOP',key),
 lRange:(key,start,end)=>transport.command('LRANGE',key,start,end),
 quit:async()=>{},
 multi(){const commands=[];const multi={lPush(key,value){commands.push(['LPUSH',key,value]);return multi;},rPop(key){commands.push(['RPOP',key]);return multi;},exec:()=>transport.transaction(commands)};return multi;},
};
const exports={};
vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'framework/infrastructure/redis/index.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Buffer,require(name){
 if(name==='@forklaunch/common')return {safeParse:JSON.parse,safeStringify:JSON.stringify};
 if(name==='@forklaunch/core/http')return {evaluateTelemetryOptions:()=>({enabled:{logging:false}})};
 if(name==='redis')return {createClient:()=>client};
 throw new Error(`Unexpected runtime import ${name}`);
}});
const actual=new exports.RedisTtlCache(3600000,{}, {url},{},{encryptor:{}});
async function native(key,operations,layout){
 const child=spawn('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-infrastructure-redis','--features','redis-client','--example','redis_queue_parity'],{cwd:root,env:{...process.env,FORKLAUNCH_TEST_REDIS_URL:url},stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify({key,operations,layout}));
 assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);return JSON.parse(output);
}
const key=()=>{const value=`fl-queue-parity:${randomUUID()}`;keys.push(value);return value;};
try{
 const queue=key();
 await actual.enqueueBatchRecords(queue,[{source:'typescript',order:1},{source:'typescript',order:2}]);
 const first=await native(queue,[{op:'peek'},{op:'peekBatch',count:10},{op:'dequeue'},{op:'enqueue',value:{source:'rust',order:3}},{op:'length'}]);
 assert.deepEqual(first,[{source:'typescript',order:1},[{source:'typescript',order:1},{source:'typescript',order:2}],{source:'typescript',order:1},null,2]);
 assert.deepEqual(JSON.parse(JSON.stringify(await actual.peekQueueRecords(queue,10))),[{source:'typescript',order:2},{source:'rust',order:3}]);
 assert.deepEqual(await actual.dequeueRecord(queue),{source:'typescript',order:2});
 await actual.enqueueRecord(queue,{source:'typescript',order:4});
 assert.deepEqual((await native(queue,[{op:'dequeueBatch',count:10}]))[0],[{source:'rust',order:3},{source:'typescript',order:4}]);
 const nativeQueue=key();
 await native(nativeQueue,[{op:'enqueueBatch',values:[{order:1},{order:2},{order:3}]}]);
 assert.deepEqual(JSON.parse(JSON.stringify(await actual.peekQueueRecord(nativeQueue))),{order:1});
 assert.deepEqual(JSON.parse(JSON.stringify(await actual.dequeueBatchRecords(nativeQueue,2))),[{order:1},{order:2}]);
 assert.deepEqual((await native(nativeQueue,[{op:'dequeue'},{op:'peekBatch',count:0}]))[0],{order:3});
 const legacy=key();await transport.command('RPUSH',legacy,JSON.stringify({order:1}),JSON.stringify({order:2}));
 assert.deepEqual(await native(legacy,[{op:'peekBatch',count:10},{op:'dequeueBatch',count:10}],'legacy'),[[{order:1},{order:2}],[{order:1},{order:2}]]);
 console.log('PASS actual TypeScript/native Redis queue interoperability: both producer directions, batch/single FIFO, non-destructive peeks and explicit legacy backlog reads');
}finally{for(const key of keys)await transport.command('DEL',key);}
