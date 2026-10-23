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
const exports={};
vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'blueprint/implementations/worker/bullmq/domain/tenantIsolation.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,process:{env:{}},console,require(name){throw new Error(`Unexpected runtime import ${name}`);}});
const cases=[{},{REDIS_KEY_PREFIX:'app:'},{FORKLAUNCH_REDIS_KEY_PREFIX:'alias:::'},{REDIS_KEY_PREFIX:'primary::',FORKLAUNCH_REDIS_KEY_PREFIX:'secondary:'},{REDIS_KEY_PREFIX:' \t',FORKLAUNCH_REDIS_KEY_PREFIX:'alias:'},{REDIS_KEY_PREFIX:'\ufeffapp::: \u2028'},{REDIS_KEY_PREFIX:'租户:'},{REDIS_KEY_PREFIX:':::',FORKLAUNCH_REDIS_KEY_PREFIX:'unused:'},{REDIS_KEY_PREFIX:'\u00a0app:\u3000'}];
const expected=cases.map(input=>{const prefix=exports.bullPrefixFor(input)||null;return {prefix,key:prefix?`${prefix}:fl_worker:{slot}:jobs`:'fl_worker:{slot}:jobs'};});
for(const input of cases){const forced=exports.withTenantPrefix({prefix:'caller-override'},input);assert.equal(forced.prefix,exports.bullPrefixFor(input)||'caller-override');}
const url=process.env.FORKLAUNCH_TEST_REDIS_URL??'redis://127.0.0.1:32770/12';
const admin=redisTransport(url),id=randomUUID().replaceAll('-',''),username=`fl_scope_${id}`,password=randomUUID(),prefix=`fl_scope_${id}`;
const restricted=new URL(url);restricted.username=username;restricted.password=password;
const commands=['ping','select','hello','quit','client','eval','evalsha','script|load','script|exists','get','set','type','time','hget','hset','hdel','hexists','hscan','incrby','zadd','zcard','zrange','zrangebyscore','zrem','subscribe','unsubscribe','publish'];
await admin.command('ACL','SETUSER',username,'reset','on',`>${password}`,'resetkeys','resetchannels',`~${prefix}:*`,`&${prefix}:*`,...commands.map(command=>`+${command}`));
try{
 const configured=await admin.command('ACL','GETUSER',username);const acl=Object.fromEntries(Array.from({length:configured.length/2},(_,index)=>[configured[index*2],configured[index*2+1]]));assert.equal(acl.channels,`&${prefix}:*`);assert.equal(acl.keys,`~${prefix}:*`);
 const env={...process.env,FORKLAUNCH_TEST_REDIS_URL:restricted.toString(),REDIS_KEY_PREFIX:`\ufeff${prefix}::: `,FORKLAUNCH_REDIS_KEY_PREFIX:'ignored-alias:'};
 const child=spawn('cargo',['run','--quiet','--manifest-path',path.join(root,'blueprint-rs/Cargo.toml'),'-p','forklaunch-worker-redis','--features','redis-client','--example','tenant_scope_parity'],{cwd:root,env,stdio:['pipe','pipe','inherit']});let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify(cases));
 assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);
 assert.deepEqual(JSON.parse(output),{cases:expected,backend:true});
 await assert.rejects(redisTransport(restricted.toString()).command('GET',`outside-${id}`));
 await assert.rejects(redisTransport(restricted.toString()).command('FLUSHDB'));
 await assert.rejects(redisTransport(restricted.toString()).command('SUBSCRIBE',`outside-${id}`));
 await assert.rejects(redisTransport(restricted.toString()).command('PUBLISH',`outside-${id}`,'pause'));
 const keys=[];let cursor='0';do{const [next,page]=await admin.command('SCAN',cursor,'MATCH',`${prefix}:*`,'COUNT',1000);keys.push(...page);cursor=String(next);}while(cursor!=='0');
 assert.ok(keys.some(key=>key.endsWith(':jobs')));assert.ok(keys.some(key=>key.endsWith(':completed')));for(const key of keys)assert.ok(key.startsWith(`${prefix}:fl_worker:{`));
 console.log('PASS actual TS/native platform prefix resolution and restricted Redis ACL: queue enqueue/claim/ack, tenant scope, Pub/Sub, denied foreign keys/channels, sanitized NOPERM and runner fail-stop without retry loops');
}finally{
 let cursor='0';do{const [next,keys]=await admin.command('SCAN',cursor,'MATCH',`${prefix}:*`,'COUNT',1000);if(keys.length)await admin.command('DEL',...keys);cursor=String(next);}while(cursor!=='0');
 await admin.command('ACL','DELUSER',username);
}
