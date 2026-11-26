// Execute actual Basic login and Basic/JWT resource callback middleware paths.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load,validator}=actualValidator(root,(specifier,file)=>{
 if(specifier.endsWith('/discriminateAuthMethod'))return {discriminateAuthMethod:async config=>({type:config.jwt?'jwt':'basic',auth:{login:config.login,decodeResource:config.decodeResource}})};
 if(specifier.endsWith('/authLogContext'))return {authLogContext:()=>({}),jwtFailureReason:()=>''};
 if(specifier.endsWith('/pinoLogger'))return {meta:value=>value};
 if(specifier==='@forklaunch/common'&&file.endsWith('auth.middleware.ts'))return {isNever:()=>{throw Error('unexpected auth mechanism');}};
});
const {parseRequestAuth}=load(path.join(root,'framework/core/src/http/middleware/request/auth.middleware.ts'));
const tokens=new Set(['','A','AA','AAA','AAAA','=','====','%%%','dXNlcjpwYXNz','dXNlcjpwYXNz=ignored','dXNlcjpwYXN','dX!Nl\tcjpwYXNz']);
for(const text of ['user:pass','user:pass:extra',':pass','user:','user','::','a:b',' a : b ','é:🔑','\u0000:\u0001','u:\ufffd']){
 const token=Buffer.from(text).toString('base64');
 for(const value of [token,token.replace(/=+$/,''),token.replace(/\+/g,'-').replace(/\//g,'_'),token.slice(0,-1),token+'ŁA',token+'𠝁',token+'=ignored',token.slice(0,2)+'!'+token.slice(2)])tokens.add(value);
}
let seed=719;
const chars='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=_-!\t\r\n';
for(let index=0;index<320;index++){
 let token=index%2===0?'dTpw':'';
 for(let n=0;n<index%31;n++){
  seed=(Math.imul(seed,1664525)+1013904223)>>>0;
  token+=chars[seed%chars.length];
 }
 tokens.add(token);
}
const cases=[{header:null,prefix:null,accepted:true}];
for(const token of tokens)for(const accepted of [false,true])cases.push({header:`Basic ${token}`,prefix:null,accepted});
for(const header of ['Basic','Basic ','basic dXNlcjpwYXNz','BASIC dXNlcjpwYXNz','Basic  dXNlcjpwYXNz',' Basic dXNlcjpwYXNz','Basic\tdXNlcjpwYXNz','Basic dXNlcjpwYXNz trailing','Custom dXNlcjpwYXNz'])for(const prefix of [null,'Custom','basic',''])cases.push({header,prefix,accepted:true});
const resourceHeaders=[null,'Basic','Basic ','Basic  opaque','Basic opaque ignored','Basic opaque:not-base64','Custom opaque','basic opaque',' Basic opaque'];
for(const header of resourceHeaders)for(const prefix of [null,'Custom','basic',''])for(const claims of [null,{},[],false,0,'',{sub:'verified',roles:['admin'],nested:{team:'one'}}])cases.push({header,prefix,accepted:false,resource:true,claims});
const jwtHeaders=[null,'Bearer','Bearer ','Bearer  opaque','Bearer opaque ignored','Bearer opaque:not-base64','Custom opaque','bearer opaque',' Bearer opaque'];
for(const header of jwtHeaders)for(const prefix of [null,'Custom','bearer',''])for(const claims of [null,{},[],false,true,0,-0,1,-1,'','verified',{sub:'verified',roles:['admin']}])for(const fail of [false,true])cases.push({header,prefix,accepted:false,resource:true,method:'jwt',claims,fail});
const expected=[];
for(const fixture of cases){
 const calls=[];
 const auth={...(fixture.resource?{decodeResource:async token=>{calls.push(token);await Promise.resolve();if(fixture.fail)throw Error('private decoder failure');return fixture.claims;}}:{}),...(fixture.method==='jwt'?{jwt:{}}:{basic:{}}),login:(user,password)=>{calls.push([user,password]);return fixture.accepted;},...(fixture.prefix===null?{}:{tokenPrefix:fixture.prefix})};
 const req={method:'GET',path:'/basic',headers:fixture.header===null?{}:{authorization:fixture.header},body:undefined,schemaValidator:validator,contractDetails:{auth,access:'authenticated'}};
 let status=200,message=null,passed=false;
 const res={type(){return this;},status(code){status=code;return this;},send(value){message=value;return this;}};
 await parseRequestAuth(req,res,()=>{passed=true;});
 assert.equal(passed,status===200);
 expected.push(fixture.resource?{status,message,claims:req.session??null,calls}:{status,message,subject:req.session?.sub??null,calls});
}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','auth','--example','typescript_basic_parity'],{cwd:root,input:JSON.stringify(cases),encoding:'utf8',timeout:120000,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'2'},maxBuffer:16*1024*1024}));
assert.equal(actual.length,cases.length);
for(const [index,result] of actual.entries())assert.deepEqual(result,expected[index],JSON.stringify(cases[index]));
console.log(`PASS ${cases.length} actual TypeScript/native Basic/JWT credential comparisons: prefix, token splitting, Node base64, UTF-8, colon fields, denied login, verified subject and async resource claims and sanitized JWT failures`);
