// Callback scheduling/validation boundaries, using actual auth middleware.
// Validator outcomes are controlled here; schema parser conformance is separate.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {actualValidator} from './rust_actual_validator_fixture.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const {load}=actualValidator(root,(specifier,file)=>{
 if(specifier.endsWith('/discriminateAuthMethod'))return {discriminateAuthMethod:async config=>({type:'jwt',auth:{decodeResource:config.decodeResource}})};
 if(specifier.endsWith('/authLogContext'))return {authLogContext:()=>({}),jwtFailureReason:()=>''};
 if(specifier.endsWith('/pinoLogger'))return {meta:value=>value};
 if(specifier==='@forklaunch/common'&&file.endsWith('auth.middleware.ts'))return {isNever:()=>{throw Error('unexpected auth mechanism');}};
});
const {parseRequestAuth}=load(path.join(root,'framework/core/src/http/middleware/request/auth.middleware.ts'));
const policies=[{}, {allowedRoles:['admin']}, {allowedPermissions:['read'],allowedRoles:['admin']},
 {allowedPermissions:[],forbiddenPermissions:['banned'],allowedRoles:['admin']},
 {requiredScope:'team'}, {requiredScope:'',scopeHeirarchy:[]},
 {requiredScope:'team',scopeHeirarchy:['team']}, {requiredFeatures:['one']},
 {requireActiveSubscription:true},
 {requiredScope:'team',scopeHeirarchy:['team'],allowedPermissions:['read'],allowedRoles:['admin'],requireActiveSubscription:true,requiredFeatures:['one']},
 {allowedRoles:[],forbiddenRoles:['blocked'],requireActiveSubscription:true,requiredFeatures:['one']},
];
const names=['scopes','permissions','roles','subscription','features'];
const fixtures=[],expected=[];
const grantSets=[{scopes:['team'],permissions:['read'],roles:['admin'],subscription:true,features:['one']},
 {scopes:['team','unknown'],permissions:['banned'],roles:['blocked'],subscription:false,features:[]}];
for(const access of [null,'public','authenticated','protected','internal'])for(const policy of policies)
for(const callbacks of [names,[],...names.map(name=>names.filter(value=>value!==name))])
for(const validation of [null,true,false])for(const grants of grantSets){
 fixtures.push({access,policy,callbacks,validation,grants,fail:null,claims:{sub:'verified',extra:{unchanged:true}}});
}
for(const fail of names)for(const access of [null,'public','authenticated','protected','internal']){
 fixtures.push({access,policy:policies[9],callbacks:names,validation:true,grants:grantSets[0],fail,claims:{sub:'verified',extra:{unchanged:true}}});
}
for(const fixture of fixtures){
 const calls=[];
 const auth={jwt:{},decodeResource:async()=>fixture.claims,...fixture.policy};
 for(const field of ['allowedPermissions','forbiddenPermissions','allowedRoles','forbiddenRoles'])if(field in auth)auth[field]=new Set(auth[field]);
 for(const name of fixture.callbacks)auth[`surface${name==='subscription'?'Subscription':name[0].toUpperCase()+name.slice(1)}`]=async claims=>{
  assert.deepEqual(claims,fixture.claims);calls.push(name);await Promise.resolve();
  if(fixture.fail===name)throw Error('public callback error');
  return name==='subscription'?fixture.grants[name]:new Set(fixture.grants[name]);
 };
 if(fixture.validation!==null)auth.sessionSchema={fixture:true};
 const schemaValidator={parse(_schema,claims){assert.deepEqual(claims,fixture.claims);calls.push('validate');return fixture.validation?{ok:true,value:{sub:'transformed'}}:{ok:false,errors:[]};}};
 const req={method:'GET',path:'/session',headers:{authorization:'Bearer verified'},schemaValidator,contractDetails:{auth,...(fixture.access===null?{}:{access:fixture.access})}};
 let status=200,message=null;
 const res={type(){return this;},status(value){status=value;return this;},send(value){message=value;return this;}};
 try{await parseRequestAuth(req,res,()=>{});}catch(error){status=500;message=error.message;}
 if(fixture.access!=='public')assert.deepEqual(req.session,fixture.claims);
 expected.push({status,message,calls});
}
const actual=JSON.parse(execFileSync('cargo',['run','--quiet','--manifest-path',path.join(root,'framework-rs/Cargo.toml'),'-p','forklaunch-core','--features','auth','--example','typescript_session_parity'],{cwd:root,input:JSON.stringify(fixtures),encoding:'utf8',timeout:240000,env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'2'},maxBuffer:32*1024*1024}));
assert.equal(actual.length,fixtures.length);
for(const [index,result] of actual.entries())assert.deepEqual(result,expected[index],JSON.stringify(fixtures[index]));
console.log(`PASS ${fixtures.length} actual TypeScript/native ordered session callback comparisons: validation, unchanged claims, missing callbacks, access, short-circuiting, precedence and async failures`);
