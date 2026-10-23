import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFile,realpath} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {Readable} from 'node:stream';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const require=createRequire(import.meta.url);
const tsc=process.env.TSC_BIN??path.join(root,'framework/node_modules/.bin/tsc');
const ts=require(require.resolve('typescript',{paths:[path.dirname(await realpath(tsc))]}));
const exports={};
const localRequire=name=>{
 if(name==='@aws-sdk/client-s3')return {S3Client:class{constructor(config){this.config=config;}}};
 if(['@aws-sdk/s3-presigned-post','@aws-sdk/lib-storage','@aws-sdk/s3-request-presigner'].includes(name))return {};
 if(name==='stream')return {Readable};
 throw new Error(`Unexpected runtime import ${name}`);
};
vm.runInNewContext(ts.transpileModule(await readFile(path.join(root,'framework/infrastructure/S3/index.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,require:localRequire,Date,Number});
const cases=[{},
 {S3_REGION:'us-east-1'},
 {S3_URL:'http://localhost:9000'},
 {S3_URL:'https://owned.example',S3_REGION:'us-west-2'},
 {S3_ACCESS_KEY_ID:'fixture-access'},
 {S3_SECRET_ACCESS_KEY:'fixture-secret-only'},
 {S3_ACCESS_KEY_ID:'fixture-access',S3_SECRET_ACCESS_KEY:'fixture-secret-only'},
 {S3_ACCESS_KEY_ID:'',S3_SECRET_ACCESS_KEY:'fixture-secret-only'},
 ...['','/','///','/tenant/owned/','tenant//owned///','租户/objects'].map(S3_PREFIX=>({S3_PREFIX})),
 ...['','0','-1','invalid','NaN','Infinity','900.9',' 600 ','\ufeff600\u2028','\u0085600','\u180e600','0x10','0o10','0b10','1e2','0.5','604800'].map(value=>({S3_PRESIGN_MAX_UPLOAD_SECONDS:value,S3_PRESIGN_MAX_DOWNLOAD_SECONDS:value})),
].map(input=>({S3_BUCKET:'native-config-fixture',...input}));
const expected=cases.map(input=>{
 const clientConfig=exports.s3ClientConfig({url:input.S3_URL,region:input.S3_REGION,accessKeyId:input.S3_ACCESS_KEY_ID,secretAccessKey:input.S3_SECRET_ACCESS_KEY});
 const store=new exports.S3ObjectStore({}, {bucket:input.S3_BUCKET,clientConfig,prefix:input.S3_PREFIX,presignLimits:{maxUploadSeconds:Number(input.S3_PRESIGN_MAX_UPLOAD_SECONDS)||undefined,maxDownloadSeconds:Number(input.S3_PRESIGN_MAX_DOWNLOAD_SECONDS)||undefined}}, {},{encryptor:{}});
 return {bucket:store.bucket,region:clientConfig.region??null,endpoint:clientConfig.endpoint??null,staticCredentials:!!clientConfig.credentials,prefix:store.prefix,uploadSeconds:store.maxUploadSeconds,downloadSeconds:store.maxDownloadSeconds};
});
const child=spawn('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-infrastructure-s3','--features','aws-sdk','--example','s3_environment_parity'],{cwd:root,env:process.env,stdio:['pipe','pipe','inherit']});
let output='';child.stdout.on('data',chunk=>output+=chunk);child.stdin.end(JSON.stringify(cases));
assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);
assert.deepEqual(JSON.parse(output),expected);
console.log(`PASS ${cases.length} actual TypeScript/native S3 environment cases: region, endpoint, complete/partial credentials, prefixes and presign limits`);
const requests=[];
const server=createServer((request,response)=>{
 requests.push({path:request.url,authorization:request.headers.authorization,token:request.headers['x-amz-security-token']});
 if(request.url==='/credentials'){
  response.setHeader('content-type','application/json');response.end(JSON.stringify({AccessKeyId:'ECSFIXTURE001',SecretAccessKey:'owned-ecs-fixture-secret-only',Token:'owned-ecs-session-token',Expiration:new Date(Date.now()+3600000).toISOString()}));
 }else if(request.url.includes('tagging')){response.setHeader('content-type','application/xml');response.end('<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><TagSet/></Tagging>');}
 else{response.setHeader('content-type','application/octet-stream');response.end('fixture');}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const base=`http://127.0.0.1:${server.address().port}`;
 const env={...Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith('AWS_')&&!name.startsWith('S3_'))),AWS_CONFIG_FILE:'/dev/null',AWS_SHARED_CREDENTIALS_FILE:'/dev/null',AWS_EC2_METADATA_DISABLED:'true',AWS_CONTAINER_CREDENTIALS_FULL_URI:`${base}/credentials`,S3_BUCKET:'native-config-fixture',S3_REGION:'us-east-1',S3_URL:base,S3_PREFIX:'/tenant/owned/'};
 const child=spawn('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-infrastructure-s3','--features','aws-sdk','--example','s3_environment_parity','--','--request'],{cwd:root,env,stdio:['ignore','pipe','inherit']});
 let output='';child.stdout.on('data',chunk=>output+=chunk);
 assert.equal(await new Promise(resolve=>child.once('close',resolve)),0);
 assert.deepEqual(JSON.parse(output),{body:[...Buffer.from('fixture')]});
 assert.ok(requests.some(request=>request.path==='/credentials'),'The official default chain must load the container role');
 const storageRequests=requests.filter(request=>request.path!=='/credentials');assert.ok(storageRequests.length>=2);
 for(const request of storageRequests){assert.ok(request.path.startsWith('/native-config-fixture/tenant/owned/probe'));assert.match(request.authorization,/Credential=ECSFIXTURE001\//);assert.equal(request.token,'owned-ecs-session-token');}
 console.log('PASS native S3 default container credential chain signs actual SDK requests and confines object keys to the configured prefix');
}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
