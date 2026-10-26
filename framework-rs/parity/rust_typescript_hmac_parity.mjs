// Run actual middleware dispatch with an observed async verifier; wire signing
// itself is covered by rust_request_hmac_parity.mjs against createHmacToken.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load,validator}=actualValidator(root,(specifier,file)=>{
 if(specifier.endsWith('/discriminateAuthMethod'))return {discriminateAuthMethod:async config=>({type:'hmac',auth:{verificationFunction:config.hmac.verify}})};
 if(specifier.endsWith('/authLogContext'))return {authLogContext:()=>({}),jwtFailureReason:()=>''};
 if(specifier.endsWith('/pinoLogger'))return {meta:value=>value};
 if(specifier==='@forklaunch/common'&&file.endsWith('auth.middleware.ts'))return {isNever:()=>{throw Error('unexpected mechanism');}};
});
const {parseRequestAuth}=load(path.join(root,'framework/core/src/http/middleware/request/auth.middleware.ts'));
const valid=['keyId=peer','ts=2026-10-13T10:00:00.000Z','nonce=nonce-one','signature=synthetic='];
const headers=new Set([null,'','HMAC','HMAC ',' HMAC '+valid.join(' '),'hmac '+valid.join(' '),'Custom '+valid.join(' '),'HMAC '+valid.join(' '),'HMAC '+valid.join(' ')+' ignored','HMAC '+valid.join('\t')]);
for(let index=0;index<4;index++)for(const value of ['',valid[index].split('=')[0],valid[index].split('=')[0]+'=',valid[index].replace('=','=extra='),'wrong=value',valid[(index+1)%4],'x','=',valid[index]+'=tail','invalid-date']) {
 const parts=[...valid];parts[index]=value;headers.add('HMAC '+parts.join(' '));
}
for(let size=0;size<4;size++)headers.add('HMAC '+valid.slice(0,size).join(' '));
headers.add('HMAC keyId=unknown ts=invalid-date nonce=n signature=s');
headers.add('HMAC keyId=peer=extra ts=raw nonce=n=extra signature=s=extra');
const cases=[];
for(const header of headers)for(const prefix of [null,'Custom','hmac',''])for(const keys of [null,{}, {peer:'synthetic-key','peer=extra':'alternate-key'}])for(const accepted of [false,true])cases.push({header,prefix,keys,accepted});
const expected=[];
for(const fixture of cases) {
 const calls=[];
 const auth={hmac:{...(fixture.keys===null?{}:{secretKeys:fixture.keys}),verify:async input=>{
  calls.push({nonce:input.nonce,signature:input.signature,secretKey:input.secretKey??null});
  await Promise.resolve();return fixture.accepted;
 }},allowedRoles:new Set(['denied']),...(fixture.prefix===null?{}:{tokenPrefix:fixture.prefix})};
 const req={method:'POST',path:'/hmac',headers:fixture.header===null?{}:{authorization:fixture.header},schemaValidator:validator,contractDetails:{auth,access:'protected'}};
 let status=200,message=null,passed=false;
 const res={type(){return this;},status(code){status=code;return this;},send(value){message=value;return this;}};
 await parseRequestAuth(req,res,()=>{passed=true;});assert.equal(passed,status===200);
 expected.push({status,message,calls,session:req.session??null});
}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','auth','--example','typescript_hmac_parity'],{cwd:root,input:JSON.stringify(cases),encoding:'utf8',timeout:120000,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'2'},maxBuffer:16*1024*1024}));
assert.equal(actual.length,cases.length);
for(const [index,result] of actual.entries())assert.deepEqual(result,expected[index],JSON.stringify(cases[index]));
console.log(`PASS ${cases.length} actual TypeScript/native HMAC header/dispatch comparisons: error ordering, prefix, fields, equals, ignored suffixes, configured key lookup, async denial and absent session`);
